//! The latency-growth simulator: a line-in room playing at L_local, a second
//! room joining, the server growing the latency, and the glitch criterion
//! (K94) checked on what the playing room hears.
//!
//! What runs, in order, for every output chunk:
//!
//! 1. The capture endpoint (the line-in, ADR 0066) captures 20 ms source
//!    chunks stamped at their capture instants; a chunk is complete one chunk
//!    after its first frame was digitized and reaches the server over the
//!    capture endpoint's link (base delay plus that link's jitter model,
//!    `crates/sync/src/jitter.rs`, delivered in order as one stream is).
//! 2. The server plans the output chunk ([`crate::latency_grow::LatencyPlan`]),
//!    waits until every source frame the plan's interpolator reads has
//!    arrived, renders it ([`crate::latency_grow::CubicResampler`]) and sends
//!    it, a fixed processing time later, to every room playing.
//! 3. Each room's endpoint receives it over its own link and must hold it a
//!    guard time before its play-at stamp; a later arrival is an underrun.
//!
//! The source signal is the frame index itself (sample `i` of the source is
//! the number `i`). Cubic interpolation reproduces a straight line exactly, so
//! every output sample the playing room hears IS the source position it
//! played, and the glitch criterion is checked on the audio, not on the plan
//! that made it: a dropped frame is a step above `1 + r_max`, a repeat a step
//! of 0, an inserted zero a step down to 0.
//!
//! Inter-room error once both rooms play comes from the house simulation's
//! own single-client runs of the two endpoints (`crate::house`), sampled at
//! each chunk's play-at instant. Both rooms render the same stamps (one plan
//! per stream), so the stamps add nothing to it by construction; what is left
//! is each endpoint's sync error against the server timeline.
//!
//! The negative control ([`Mode::NaiveJump`]) is the obvious alternative:
//! no stretch, the latency set to L_group at once. It is run through the same
//! checks so a test can show the criterion tells the two apart.
//!
//! Everything here is a simulation and NOT timing evidence (BRIEF.md section
//! 3.1 rule 3). The scenario files are under `config/sim-latency/`, not
//! `fixtures/sync/`, for the reason houses are (ADR 0048): the endpoint's C
//! core models one endpoint and has nothing to read them with.

use std::fmt;

use crate::house::{HouseConfig, CROSS_ROOM_ACCEPTABLE_NS};
use crate::jitter::{JitterModel, JitterProcess};
use crate::latency_grow::{CubicResampler, GrowthConfig, LatencyPlan, PlanError, LOOKAHEAD_FRAMES};
use crate::rng::Rng;
use crate::sim::{run, SimResult};

/// Seeds the capture endpoint's upstream jitter ("uplink").
const UPLINK_STREAM: u64 = 0x7570_6C69_6E6B_0000;
/// Seeds the downstream jitter to the playing room.
const PLAY_STREAM: u64 = 0x706C_6179_0000_0000;
/// Seeds the downstream jitter to the joining room.
const JOIN_STREAM: u64 = 0x6A6F_696E_0000_0000;

/// The committed scenarios, in report order, by file name under
/// `config/sim-latency/`.
pub const SCENARIOS: [&str; 2] = ["wired-join.latency", "wifi-join.latency"];

/// One scenario, as read from a `config/sim-latency/*.latency` file.
#[derive(Debug, Clone, PartialEq)]
pub struct LatencyScenario {
    /// Name, from the file.
    pub name: String,
    /// The house file (under `config/sim-house/`) the endpoints come from.
    pub house: String,
    /// The endpoint whose line-in is the source; it plays in its own room.
    pub source: String,
    /// The endpoint of the room that joins.
    pub joiner: String,
    /// Seeds the chunk transport's jitter streams.
    pub seed: u64,
    /// Run length, in ms of server timeline.
    pub duration_ms: u64,
    /// The stream's sample rate, in Hz.
    pub sample_rate_hz: u32,
    /// Chunk length, source and output alike, in ms.
    pub chunk_ms: u32,
    /// When the server learns the second room joins, in ms.
    pub join_at_ms: u64,
    /// When it leaves again, in ms; `None` for never.
    pub leave_at_ms: Option<u64>,
    /// The latency the source room plays at alone, in us.
    pub l_local_us: u64,
    /// The latency the group is held to, in us.
    pub l_group_us: u64,
    /// The least latency the joining room's tier starts at, in us.
    pub join_min_latency_us: u64,
    /// The largest rate deviation, in ppm.
    pub max_rate_ppm: f64,
    /// Each raised-cosine ramp's length, in ms.
    pub ramp_ms: u64,
    /// The server's time from the last source frame arriving to the output
    /// chunk leaving, in us.
    pub server_processing_us: u64,
    /// How long before its play-at stamp an endpoint must hold a chunk, in us.
    pub endpoint_guard_us: u64,
}

/// Why a scenario could not be used.
#[derive(Debug, Clone, PartialEq)]
pub enum ScenarioFileError {
    /// A line is not `key = value`, or a key is unknown, twice, or missing.
    Malformed(String),
    /// A value breaks a rule (see [`LatencyScenario::validate`]).
    Invalid(String),
}

impl fmt::Display for ScenarioFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScenarioFileError::Malformed(why) => write!(f, "malformed: {}", why),
            ScenarioFileError::Invalid(why) => write!(f, "invalid: {}", why),
        }
    }
}

impl std::error::Error for ScenarioFileError {}

const KEYS: [&str; 17] = [
    "name",
    "house",
    "source",
    "joiner",
    "seed",
    "duration_ms",
    "sample_rate_hz",
    "chunk_ms",
    "join_at_ms",
    "leave_at_ms",
    "l_local_us",
    "l_group_us",
    "join_min_latency_us",
    "max_rate_ppm",
    "ramp_ms",
    "server_processing_us",
    "endpoint_guard_us",
];

impl LatencyScenario {
    /// Parse a scenario file: `key = value` lines, `#` comments, every key in
    /// the list above exactly once (`leave_at_ms` may be `never`).
    pub fn parse(text: &str) -> Result<LatencyScenario, ScenarioFileError> {
        let bad = |why: String| ScenarioFileError::Malformed(why);
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| bad(format!("line {}: not `key = value`", index + 1)))?;
            let (key, value) = (key.trim(), value.trim());
            if !KEYS.contains(&key) {
                return Err(bad(format!("line {}: unknown key {:?}", index + 1, key)));
            }
            if pairs.iter().any(|(k, _)| k == key) {
                return Err(bad(format!("{} appears twice", key)));
            }
            pairs.push((key.to_string(), value.to_string()));
        }
        let get = |key: &str| -> Result<&str, ScenarioFileError> {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .ok_or_else(|| bad(format!("{} is required", key)))
        };
        let whole = |key: &str| -> Result<u64, ScenarioFileError> {
            get(key)?
                .parse::<u64>()
                .map_err(|_| bad(format!("{} is not an unsigned whole number", key)))
        };
        let leave_at_ms = match get("leave_at_ms")? {
            "never" => None,
            _ => Some(whole("leave_at_ms")?),
        };
        let max_rate_ppm = get("max_rate_ppm")?
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| bad("max_rate_ppm is not a finite number".to_string()))?;
        let narrow = |key: &str| -> Result<u32, ScenarioFileError> {
            u32::try_from(whole(key)?).map_err(|_| bad(format!("{} is too large", key)))
        };
        let scenario = LatencyScenario {
            name: get("name")?.to_string(),
            house: get("house")?.to_string(),
            source: get("source")?.to_string(),
            joiner: get("joiner")?.to_string(),
            seed: whole("seed")?,
            duration_ms: whole("duration_ms")?,
            sample_rate_hz: narrow("sample_rate_hz")?,
            chunk_ms: narrow("chunk_ms")?,
            join_at_ms: whole("join_at_ms")?,
            leave_at_ms,
            l_local_us: whole("l_local_us")?,
            l_group_us: whole("l_group_us")?,
            join_min_latency_us: whole("join_min_latency_us")?,
            max_rate_ppm,
            ramp_ms: whole("ramp_ms")?,
            server_processing_us: whole("server_processing_us")?,
            endpoint_guard_us: whole("endpoint_guard_us")?,
        };
        scenario.growth_config()?;
        Ok(scenario)
    }

    /// The plan configuration the scenario implies, after checking the
    /// scenario's own rules: a chunk is a whole number of frames, the group
    /// latency is above the local one and the joining tier's start, the
    /// join comes before any leave and both inside the run.
    pub fn growth_config(&self) -> Result<GrowthConfig, ScenarioFileError> {
        let invalid = |why: &str| Err(ScenarioFileError::Invalid(why.to_string()));
        let frames = u64::from(self.sample_rate_hz) * u64::from(self.chunk_ms);
        if self.chunk_ms == 0 || frames % 1_000 != 0 {
            return invalid("chunk_ms is not a whole, nonzero number of frames");
        }
        if self.l_group_us <= self.l_local_us {
            return invalid("l_group_us is not above l_local_us");
        }
        if self.join_min_latency_us > self.l_group_us {
            return invalid("join_min_latency_us is above l_group_us");
        }
        if self.join_at_ms >= self.duration_ms {
            return invalid("join_at_ms is not inside the run");
        }
        if let Some(leave) = self.leave_at_ms {
            if leave <= self.join_at_ms || leave >= self.duration_ms {
                return invalid("leave_at_ms is not between the join and the end");
            }
        }
        if self.source == self.joiner {
            return invalid("the joining endpoint is the source endpoint");
        }
        let config = GrowthConfig {
            sample_rate_hz: self.sample_rate_hz,
            chunk_frames: (frames / 1_000) as u32,
            max_rate_deviation: self.max_rate_ppm * 1e-6,
            ramp_frames: u64::from(self.sample_rate_hz) * self.ramp_ms / 1_000,
        };
        config
            .validate()
            .map_err(|e| ScenarioFileError::Invalid(e.to_string()))?;
        Ok(config)
    }
}

/// How the server moves the latency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The plan of [`crate::latency_grow`]: a bounded, smooth time stretch.
    Stretch,
    /// The negative control: the play-at offset set to the new target at
    /// once, the source played at its own rate.
    NaiveJump,
}

/// One output chunk as the run saw it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkRecord {
    /// Index from the stream's first.
    pub index: u64,
    /// Play-at stamp, server timeline, in ns.
    pub play_at_ns: i64,
    /// Play-at minus the capture instant of its first source frame, in ns.
    pub offset_ns: f64,
    /// Rate deviation at its first frame.
    pub rate_start: f64,
    /// Source frames it consumed.
    pub source_frames: f64,
    /// When the server sent it, server timeline, in ns.
    pub sent_ns: f64,
    /// When the playing room had it.
    pub play_arrival_ns: f64,
    /// When the joining room had it, if the joining room was sent it.
    pub join_arrival_ns: Option<f64>,
    /// Whether the joining room played it.
    pub join_plays: bool,
}

/// The glitch criterion's measurements for the playing room.
#[derive(Debug, Clone, PartialEq)]
pub struct GlitchReport {
    /// Output frames the playing room's timeline covered (gaps included).
    pub frames_checked: u64,
    /// Smallest step between consecutive output samples, read as source
    /// positions.
    pub min_step: f64,
    /// Largest such step.
    pub max_step: f64,
    /// Output frames with no audio to play (zeros inserted).
    pub inserted_frames: u64,
    /// Output frames the timeline skipped back over (audio dropped).
    pub overlapped_frames: u64,
    /// Largest |rate deviation| at any frame.
    pub max_abs_rate: f64,
    /// Largest change of the rate deviation from one chunk to the next.
    pub max_rate_change_per_chunk: f64,
    /// Largest |rendered sample - planned position|.
    pub max_render_error: f64,
    /// Chunks that reached the playing room later than the guard allows.
    pub underruns: u64,
    /// Chunks whose offset moved away from where the plan was heading.
    pub offset_reversals: u64,
}

/// What one transition did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    /// The latency it started from, in ns.
    pub from_ns: f64,
    /// The latency it was asked for, in ns.
    pub to_ns: f64,
    /// The chunk whose plan first saw the request.
    pub requested_chunk: u64,
    /// The first chunk played at the new latency, if one was.
    pub reached_chunk: Option<u64>,
}

/// Distribution of |inter-room error| while both rooms play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairStats {
    /// Chunks both rooms played.
    pub samples: usize,
    /// Median, nearest rank, ns.
    pub p50_ns: i64,
    /// 95th percentile, nearest rank, ns.
    pub p95_ns: i64,
    /// Largest, ns.
    pub max_ns: i64,
}

/// Everything a run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct LatencyRun {
    /// How the latency was moved.
    pub mode: Mode,
    /// The plan configuration.
    pub config: GrowthConfig,
    /// Every output chunk.
    pub chunks: Vec<ChunkRecord>,
    /// The playing room's criterion measurements.
    pub glitch: GlitchReport,
    /// The growth, then (if the room left) the shrink.
    pub transitions: Vec<Transition>,
    /// The first chunk the joining room played.
    pub join_start_chunk: Option<u64>,
    /// Chunks the joining room received late after it started.
    pub join_underruns: u64,
    /// Inter-room error while both played.
    pub pair: Option<PairStats>,
}

impl LatencyRun {
    /// The glitch criterion of the decision record, for the playing room and
    /// the pair: every reason it fails, empty when it passes.
    ///
    /// (a) output positions strictly increase with every step inside
    /// [1 - r_max, 1 + r_max], no frame inserted or dropped; (b) no underrun;
    /// (c) the offset never moves away from where the plan heads, and every
    /// transition reaches its target; (d) the rate's change per chunk is
    /// inside the profile's slope bound; and, for the pair, the inter-room
    /// error under BRIEF.md section 2.2's cross-room acceptable bound.
    pub fn failures(&self) -> Vec<String> {
        let mut out = Vec::new();
        let g = &self.glitch;
        let r = self.config.max_rate_deviation;
        // A float allowance for the step check: positions run to about 1e8
        // frames, where one ulp is about 1.5e-8.
        let eps = 1e-6;
        if g.inserted_frames > 0 {
            out.push(format!("(a) {} silent frames inserted", g.inserted_frames));
        }
        if g.overlapped_frames > 0 {
            out.push(format!("(a) {} frames dropped", g.overlapped_frames));
        }
        if g.min_step < 1.0 - r - eps || g.max_step > 1.0 + r + eps {
            out.push(format!(
                "(a) a step of {:.9} or {:.9} frames is outside [1 - r_max, 1 + r_max]",
                g.min_step, g.max_step
            ));
        }
        if g.max_abs_rate > r * (1.0 + 1e-9) {
            out.push(format!("(a) the rate reached {:e}", g.max_abs_rate));
        }
        if g.max_render_error > eps {
            out.push(format!(
                "(a) the rendered audio is {:e} frames off the plan",
                g.max_render_error
            ));
        }
        if g.underruns > 0 {
            out.push(format!("(b) {} underruns in the playing room", g.underruns));
        }
        if g.offset_reversals > 0 {
            out.push(format!(
                "(c) the offset moved away from its target {} times",
                g.offset_reversals
            ));
        }
        for t in &self.transitions {
            if t.reached_chunk.is_none() {
                out.push(format!(
                    "(c) the transition to {:.3} ms never arrived",
                    t.to_ns / 1e6
                ));
            }
        }
        let slope = self.config.max_rate_change_per_chunk() * (1.0 + 1e-9);
        if g.max_rate_change_per_chunk > slope {
            out.push(format!(
                "(d) the rate changed by {:e} in one chunk (bound {:e})",
                g.max_rate_change_per_chunk, slope
            ));
        }
        match self.pair {
            Some(p) if p.max_ns >= CROSS_ROOM_ACCEPTABLE_NS => out.push(format!(
                "pair: inter-room error reached {} ns (bound {} ns)",
                p.max_ns, CROSS_ROOM_ACCEPTABLE_NS
            )),
            None => out.push("pair: the joining room never played".to_string()),
            _ => {}
        }
        out
    }
}

/// Why a run could not be made.
#[derive(Debug, Clone, PartialEq)]
pub enum LatencySimError {
    /// The scenario is unusable.
    Scenario(ScenarioFileError),
    /// An endpoint it names is not in the house.
    NoEndpoint(String),
    /// The house's single-client run refused an endpoint.
    Endpoint(String),
    /// The plan refused something.
    Plan(PlanError),
}

impl fmt::Display for LatencySimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LatencySimError::Scenario(e) => write!(f, "scenario: {}", e),
            LatencySimError::NoEndpoint(id) => write!(f, "endpoint {} is not in the house", id),
            LatencySimError::Endpoint(why) => write!(f, "endpoint run: {}", why),
            LatencySimError::Plan(e) => write!(f, "plan: {}", e),
        }
    }
}

impl std::error::Error for LatencySimError {}

impl From<PlanError> for LatencySimError {
    fn from(e: PlanError) -> LatencySimError {
        LatencySimError::Plan(e)
    }
}

/// One endpoint's network path for chunks, and the clock-sync run that says
/// how far its playout is from the server timeline.
struct Path {
    base_ns: f64,
    jitter: JitterProcess,
    rng: Rng,
    last_arrival_ns: f64,
}

impl Path {
    fn new(base_us: f64, jitter: JitterModel, seed: u64) -> Path {
        Path {
            base_ns: base_us * 1_000.0,
            jitter: JitterProcess::new(jitter),
            rng: Rng::new(seed),
            last_arrival_ns: f64::MIN,
        }
    }

    /// When something sent at `sent_ns` arrives: one stream, so in order.
    fn deliver(&mut self, sent_ns: f64) -> f64 {
        let raw = sent_ns + self.base_ns + self.jitter.sample_ns(&mut self.rng);
        self.last_arrival_ns = self.last_arrival_ns.max(raw);
        self.last_arrival_ns
    }
}

fn endpoint_index(house: &HouseConfig, id: &str) -> Result<usize, LatencySimError> {
    house
        .endpoints
        .iter()
        .position(|e| e.id == id)
        .ok_or_else(|| LatencySimError::NoEndpoint(id.to_string()))
}

fn sync_run(
    house: &HouseConfig,
    index: usize,
    duration_ms: u64,
) -> Result<SimResult, LatencySimError> {
    let mut config = house.endpoint_config(index);
    config.duration_ms = duration_ms;
    run(&config)
        .map_err(|e| LatencySimError::Endpoint(format!("{}: {}", house.endpoints[index].id, e)))
}

/// Error of a single-client run at server time `t_ns`, from its step grid.
fn error_at(result: &SimResult, step_ns: f64, server_ppm: f64, t_ns: f64) -> i64 {
    let true_ns = t_ns / (1.0 + server_ppm * 1e-6);
    // Sample i is at true time (i + 1) steps.
    let index = ((true_ns / step_ns).floor() as usize).saturating_sub(1);
    result.samples[index.min(result.samples.len() - 1)].error_ns
}

/// Nearest-rank percentile of `values`, which it reorders.
fn nearest_rank(values: &mut [i64], p: f64) -> i64 {
    let rank = ((p * values.len() as f64).ceil() as usize).clamp(1, values.len());
    *values.select_nth_unstable(rank - 1).1
}

/// Run a scenario in a house.
pub fn run_latency(
    scenario: &LatencyScenario,
    house: &HouseConfig,
    mode: Mode,
) -> Result<LatencyRun, LatencySimError> {
    let config = scenario
        .growth_config()
        .map_err(LatencySimError::Scenario)?;
    let source = endpoint_index(house, &scenario.source)?;
    let joiner = endpoint_index(house, &scenario.joiner)?;
    // A second past the run's end: the last chunks play a latency after it.
    let play_sync = sync_run(house, source, scenario.duration_ms + 1_000)?;
    let join_sync = sync_run(house, joiner, scenario.duration_ms + 1_000)?;
    let step_ns = house.step_ms as f64 * 1e6;

    let n = config.chunk_frames;
    let frame_ns = config.frame_ns();
    let chunk_ns = f64::from(n) * frame_ns;
    let l_local_ns = scenario.l_local_us as i64 * 1_000;
    let l_group_ns = scenario.l_group_us as i64 * 1_000;
    let guard_ns = scenario.endpoint_guard_us as f64 * 1_000.0;
    let processing_ns = scenario.server_processing_us as f64 * 1_000.0;
    let join_at_ns = scenario.join_at_ms as f64 * 1e6;
    let leave_at_ns = scenario.leave_at_ms.map(|ms| ms as f64 * 1e6);
    let join_min_ns = scenario.join_min_latency_us as f64 * 1_000.0;

    // Uplink is the capture endpoint's forward path, downlink each player's
    // return path (forward plus its asymmetry), as the single-client model
    // defines them.
    let source_cfg = house.endpoint_config(source);
    let joiner_cfg = house.endpoint_config(joiner);
    let mut uplink = Path::new(
        source_cfg.base_one_way_delay_us,
        source_cfg.jitter,
        scenario.seed ^ UPLINK_STREAM,
    );
    let mut play_down = Path::new(
        source_cfg.base_one_way_delay_us + source_cfg.path_asymmetry_us,
        source_cfg.jitter,
        scenario.seed ^ PLAY_STREAM,
    );
    let mut join_down = Path::new(
        joiner_cfg.base_one_way_delay_us + joiner_cfg.path_asymmetry_us,
        joiner_cfg.jitter,
        scenario.seed ^ JOIN_STREAM,
    );

    // Source chunk m is complete one chunk after its first frame and reaches
    // the server in order; arrivals are drawn as far as the plan needs.
    let mut source_arrivals: Vec<f64> = Vec::new();
    let mut source_arrival = |m: usize, uplink: &mut Path| -> f64 {
        while source_arrivals.len() <= m {
            let complete = (source_arrivals.len() as f64 + 1.0) * chunk_ns;
            source_arrivals.push(uplink.deliver(complete));
        }
        source_arrivals[m]
    };

    let mut plan = LatencyPlan::new(config, 0, l_local_ns)?;
    let mut resampler = CubicResampler::new(1);
    let mut fed: u64 = 0;
    let mut out: Vec<f64> = Vec::new();

    let total_chunks = (scenario.duration_ms as f64 * 1e6 / chunk_ns) as u64;
    let mut chunks = Vec::with_capacity(total_chunks as usize);
    let mut glitch = GlitchReport {
        frames_checked: 0,
        min_step: f64::MAX,
        max_step: f64::MIN,
        inserted_frames: 0,
        overlapped_frames: 0,
        max_abs_rate: 0.0,
        max_rate_change_per_chunk: 0.0,
        max_render_error: 0.0,
        underruns: 0,
        offset_reversals: 0,
    };
    let mut transitions: Vec<Transition> = Vec::new();
    let mut joined = false;
    let mut left = false;
    let mut join_playing = false;
    let mut join_start_chunk = None;
    let mut join_underruns = 0u64;
    let mut pair_errors: Vec<i64> = Vec::new();

    // The naive control's state: the latency in force, set at once.
    let mut naive_latency_ns = l_local_ns;

    let mut last_sample: Option<f64> = None;
    let mut expected_play_at: Option<i64> = None;
    let mut last_rate: Option<f64> = None;
    let mut last_offset: Option<f64> = None;
    let mut last_sent = f64::MIN;

    for k in 0..total_chunks {
        // The server decides at the time it sent the previous chunk.
        let now = last_sent;
        let mut request = None;
        if !joined && now >= join_at_ns {
            joined = true;
            request = Some(l_group_ns);
        }
        if joined && !left && leave_at_ns.is_some_and(|t| now >= t) {
            left = true;
            join_playing = false;
            request = Some(l_local_ns);
        }

        // Plan the chunk: its stamp, its source frames, its samples.
        let (play_at_ns, offset_ns, rate_start, source_start, source_end, heading_ns, needed) =
            match mode {
                Mode::Stretch => {
                    if let Some(target) = request {
                        plan.set_target(target)?;
                    }
                    let chunk = plan.next_chunk();
                    while fed < chunk.source_frames_needed() {
                        let block: Vec<f64> = (fed..fed + u64::from(n)).map(|i| i as f64).collect();
                        resampler.push(&block);
                        fed += u64::from(n);
                    }
                    resampler.render(&chunk, &mut out)?;
                    for i in 0..chunk.frames {
                        let want = chunk.source_position(i).max(0.0);
                        glitch.max_render_error =
                            glitch.max_render_error.max((out[i as usize] - want).abs());
                        glitch.max_abs_rate = glitch.max_abs_rate.max(chunk.rate(i).abs());
                    }
                    (
                        chunk.play_at_ns,
                        chunk.offset_ns,
                        chunk.rate_start,
                        chunk.source_start,
                        chunk.source_end,
                        plan.heading_ns(),
                        chunk.source_frames_needed(),
                    )
                }
                Mode::NaiveJump => {
                    if let Some(target) = request {
                        naive_latency_ns = target;
                    }
                    // The same chunk grid as the plan's, played at rate 1.
                    let a = u64::from(LOOKAHEAD_FRAMES);
                    let first = if k == 0 { 0 } else { k * u64::from(n) - a };
                    let end = (k + 1) * u64::from(n) - a;
                    out.clear();
                    out.extend((first..end).map(|i| i as f64));
                    let play_at = (first as f64 * frame_ns).round() as i64 + naive_latency_ns;
                    (
                        play_at,
                        naive_latency_ns as f64,
                        0.0,
                        first as f64,
                        end as f64,
                        naive_latency_ns as f64,
                        end,
                    )
                }
            };

        if let Some(t) = request {
            let from = last_offset.unwrap_or(offset_ns);
            transitions.push(Transition {
                from_ns: from,
                to_ns: t as f64,
                requested_chunk: k,
                reached_chunk: None,
            });
        }
        if let Some(t) = transitions.last_mut() {
            if t.reached_chunk.is_none() && (offset_ns - t.to_ns).abs() < 1.0 {
                t.reached_chunk = Some(k);
            }
        }

        // (c): the offset never moves away from where the plan heads. The
        // naive control heads straight to its target, so a jump counts as
        // moving toward it; its failure is (a)'s.
        if let Some(prev) = last_offset {
            // Stamps are whole ns: an allowance of one for their rounding.
            if (offset_ns - heading_ns).abs() > (prev - heading_ns).abs() + 1.0 {
                glitch.offset_reversals += 1;
            }
        }
        last_offset = Some(offset_ns);
        // (d)
        if let Some(prev) = last_rate {
            glitch.max_rate_change_per_chunk = glitch
                .max_rate_change_per_chunk
                .max((rate_start - prev).abs());
        }
        last_rate = Some(rate_start);

        // (a), on the playing room's timeline: a stamp later than one chunk
        // after the last leaves silence; earlier, the timeline overlaps and
        // audio is dropped. Then the samples themselves, read as positions.
        if let Some(expected) = expected_play_at {
            // In frames, so the stamps' integer rounding cannot count as one.
            let gap = ((play_at_ns - expected) as f64 / frame_ns).round() as i64;
            if gap > 0 {
                let frames = gap as u64;
                glitch.inserted_frames += frames;
                glitch.frames_checked += frames;
                // The silence is zeros: the step into it, and out of it.
                if let Some(last) = last_sample {
                    glitch.min_step = glitch.min_step.min(0.0 - last);
                }
                last_sample = Some(0.0);
            } else if gap < 0 {
                glitch.overlapped_frames += (-gap) as u64;
            }
        }
        expected_play_at = Some(play_at_ns + (out.len() as f64 * frame_ns).round() as i64);
        for &sample in &out {
            if let Some(last) = last_sample {
                let step = sample - last;
                glitch.min_step = glitch.min_step.min(step);
                glitch.max_step = glitch.max_step.max(step);
            }
            last_sample = Some(sample);
        }
        glitch.frames_checked += out.len() as u64;

        // (b): when every source frame the chunk reads has reached the server,
        // it is rendered, sent, and delivered.
        let last_source_chunk = ((needed - 1) / u64::from(n)) as usize;
        let ready = source_arrival(last_source_chunk, &mut uplink);
        let sent = ready.max(last_sent) + processing_ns;
        last_sent = sent;
        let play_arrival = play_down.deliver(sent);
        if play_arrival > play_at_ns as f64 - guard_ns {
            glitch.underruns += 1;
        }

        // The joining room is sent every chunk from the join until it leaves,
        // and starts with the first one that both arrives in time and is
        // played at a latency its tier can start at.
        let mut join_arrival = None;
        let mut join_plays = false;
        if joined && !left {
            let arrival = join_down.deliver(sent);
            join_arrival = Some(arrival);
            let in_time = arrival <= play_at_ns as f64 - guard_ns;
            if join_playing {
                join_plays = true;
                if !in_time {
                    join_underruns += 1;
                }
            } else if in_time && offset_ns >= join_min_ns - 1.0 {
                join_playing = true;
                join_plays = true;
                join_start_chunk.get_or_insert(k);
            }
        }
        if join_plays {
            let t = play_at_ns as f64;
            let e_play = error_at(&play_sync, step_ns, house.server_ppm, t);
            let e_join = error_at(&join_sync, step_ns, house.server_ppm, t);
            pair_errors.push((e_play - e_join).saturating_abs());
        }

        chunks.push(ChunkRecord {
            index: k,
            play_at_ns,
            offset_ns,
            rate_start,
            source_frames: source_end - source_start,
            sent_ns: sent,
            play_arrival_ns: play_arrival,
            join_arrival_ns: join_arrival,
            join_plays,
        });
    }

    let pair = if pair_errors.is_empty() {
        None
    } else {
        let max_ns = pair_errors.iter().copied().max().unwrap_or(0);
        let p95_ns = nearest_rank(&mut pair_errors, 0.95);
        let p50_ns = nearest_rank(&mut pair_errors, 0.50);
        Some(PairStats {
            samples: pair_errors.len(),
            p50_ns,
            p95_ns,
            max_ns,
        })
    };

    Ok(LatencyRun {
        mode,
        config,
        chunks,
        glitch,
        transitions,
        join_start_chunk,
        join_underruns,
        pair,
    })
}

#[cfg(test)]
mod tests {
    use super::{LatencyScenario, ScenarioFileError};

    const TEXT: &str = "\
name = t
house = 8-rooms-switched.house
source = kitchen-l
joiner = office-l
seed = 1
duration_ms = 100000
sample_rate_hz = 48000
chunk_ms = 20
join_at_ms = 60000
leave_at_ms = never
l_local_us = 30000
l_group_us = 40000
join_min_latency_us = 30000
max_rate_ppm = 500
ramp_ms = 1000
server_processing_us = 1000
endpoint_guard_us = 4000
";

    #[test]
    fn a_scenario_parses() {
        let s = LatencyScenario::parse(TEXT).expect("parses");
        assert_eq!(s.leave_at_ms, None);
        let c = s.growth_config().expect("valid");
        assert_eq!(c.chunk_frames, 960);
        assert_eq!(c.ramp_frames, 48_000);
    }

    #[test]
    fn a_scenario_that_shrinks_the_latency_is_refused() {
        let text = TEXT.replace("l_group_us = 40000", "l_group_us = 20000");
        assert!(matches!(
            LatencyScenario::parse(&text),
            Err(ScenarioFileError::Invalid(_))
        ));
    }

    #[test]
    fn an_unknown_or_repeated_key_is_refused() {
        assert!(LatencyScenario::parse(&format!("{}sede = 2\n", TEXT)).is_err());
        assert!(LatencyScenario::parse(&format!("{}seed = 2\n", TEXT)).is_err());
    }
}
