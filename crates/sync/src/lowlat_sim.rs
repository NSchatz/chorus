//! The low-latency TV path's simulator (goal 13): capture, the hub, the
//! server's relay and a wired endpoint, with packet loss on both UDP legs,
//! the real FEC from `chorus_protocol::v2::lowlat`, and the budget the
//! defaults promise checked chunk by chunk.
//!
//! What runs, for every chunk `n` of a scenario (stamp `s_n = n x chunk`,
//! the capture instant of its first frame on the server timeline):
//!
//! 1. The hub has the chunk one chunk duration and the capture buffering
//!    after its stamp, numbers it with the hub's [`FecEncoder`] and sends it
//!    (and, when it completes a group, the group's parity right after it) up
//!    the first leg.
//! 2. Each datagram of a leg is lost or delivered by the leg's loss model
//!    (none, Bernoulli, or Gilbert-Elliott bursts), and a delivered one
//!    arrives the leg's base delay plus a jitter draw later
//!    (`crate::jitter`), but never before the datagram sent ahead of it (one
//!    path, first in first out).
//! 3. The server pushes arrivals through a [`FecDecoder`] in arrival order;
//!    each chunk it hands on (received, or rebuilt the instant the group's
//!    last needed datagram arrived) is restamped (`play_at = s_n + L_tv`) and
//!    sent down the second leg a relay time later through a [`FecRelay`],
//!    which keeps the hub's groups, so the FEC wait is paid once end to end.
//! 4. The endpoint decodes the same way. A chunk it has by `play_at` minus
//!    its output path (buffer, DSP block, DAC filter) plays on time; one that
//!    arrives after is late and dropped (never played past its playout
//!    point); one that never arrives is lost. Late and lost are the residual
//!    loss, heard as a 2.5 ms crossfade to silence.
//!
//! Every scenario runs twice on the same draws: with its FEC, and without
//! (`fec_k` 0), so the residual loss the FEC removes is measured against its
//! own negative control. The data datagrams' fates are drawn per chunk from
//! one stream and the parity's per group from another, and a burst model's
//! state advances once per chunk, so both runs lose exactly the same chunks
//! and differ only by the parity.
//!
//! The latency budget (`chorus_protocol::v2::lowlat::Plan`) is what the
//! defaults promise, each figure with its source; the lip-sync verdict adds
//! what happens before the stamp (the TV's own audio output against its
//! picture, a scenario parameter that is ASSUMED until the owner's TVs are
//! known, and the S/PDIF receiver) and judges the sum against BRIEF.md
//! section 2.2's window: audio within 40 ms of the picture, never leading by
//! more than 15 ms (ITU-R BT.1359-1's sign: positive is sound leading).
//!
//! Everything here is a simulation and NOT timing evidence (BRIEF.md section
//! 3.1 rule 3): the loss rates, delays and output path are ASSUMED, and the
//! report says so. The scenario files are `config/sim-lowlat/*.lowlat`,
//! Rust-only for the reason the houses are (ADR 0048).

use std::fmt;

use chorus_protocol::v2::lowlat::{
    FecDecoder, FecEncoder, FecParams, FecRelay, Kind, Plan, DEFAULTS,
};

use crate::jitter::JitterModel;
use crate::rng::Rng;

/// The committed scenarios, in report order, by file name under
/// `config/sim-lowlat/`.
pub const SCENARIOS: [&str; 6] = [
    "wired-clean.lowlat",
    "wired-bernoulli.lowlat",
    "wired-bernoulli-nofec.lowlat",
    "wired-bursts.lowlat",
    "wired-bursts-interleaved.lowlat",
    "wired-game-mode-tv.lowlat",
];

/// BRIEF.md section 2.2's lip-sync window, in ns: sound lagging the picture
/// by at most 40 ms, leading it by at most 15 ms.
pub const LIP_SYNC_WINDOW_NS: (i64, i64) = (-40_000_000, 15_000_000);

/// The Needs item every TV-dependent value names.
pub const TV_NEEDS_ITEM: &str = "The three TVs: model, eARC port, optical out and audio menu";

/// Seeds: the data datagrams' fates on the hub's leg.
const UP_DATA: u64 = 0x6C6C_7570_6461_7461;
/// Seeds: the parity datagrams' fates on the hub's leg.
const UP_PARITY: u64 = 0x6C6C_7570_7061_7269;
/// Seeds: the data datagrams' fates on the endpoint's leg.
const DOWN_DATA: u64 = 0x6C6C_646E_6461_7461;
/// Seeds: the parity datagrams' fates on the endpoint's leg.
const DOWN_PARITY: u64 = 0x6C6C_646E_7061_7269;

/// How a leg loses datagrams.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LossModel {
    /// Nothing is lost.
    None,
    /// Each datagram is lost with probability `p`, independently.
    Bernoulli {
        /// The loss probability.
        p: f64,
    },
    /// Gilbert-Elliott: a good and a bad state, each with its own loss
    /// probability; the state moves good to bad with `p_gb` and bad to good
    /// with `p_bg` once per chunk (E. N. Gilbert, BSTJ 39(5), 1960; E. O.
    /// Elliott, BSTJ 42(5), 1963; as cited by Hasslinger and Hohlfeld 2008).
    /// Mean burst `1 / p_bg` chunks; loss rate
    /// `(p_gb x loss_bad + p_bg x loss_good) / (p_gb + p_bg)`.
    Gilbert {
        /// Good to bad, per chunk.
        p_gb: f64,
        /// Bad to good, per chunk.
        p_bg: f64,
        /// Loss probability in the good state.
        loss_good: f64,
        /// Loss probability in the bad state.
        loss_bad: f64,
    },
}

impl LossModel {
    /// The long-run loss probability per datagram.
    pub fn rate(&self) -> f64 {
        match *self {
            LossModel::None => 0.0,
            LossModel::Bernoulli { p } => p,
            LossModel::Gilbert {
                p_gb,
                p_bg,
                loss_good,
                loss_bad,
            } => (p_gb * loss_bad + p_bg * loss_good) / (p_gb + p_bg),
        }
    }

    /// As the scenario file writes it.
    pub fn describe(&self) -> String {
        match *self {
            LossModel::None => "none".to_string(),
            LossModel::Bernoulli { p } => format!("bernoulli p={}", p),
            LossModel::Gilbert {
                p_gb,
                p_bg,
                loss_good,
                loss_bad,
            } => format!(
                "gilbert-elliott p_gb={} p_bg={} loss_good={} loss_bad={} (mean burst {:.1} chunks)",
                p_gb,
                p_bg,
                loss_good,
                loss_bad,
                1.0 / p_bg
            ),
        }
    }

    fn parse(text: &str) -> Option<LossModel> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let num = |i: usize| -> Option<f64> {
            words
                .get(i)?
                .parse::<f64>()
                .ok()
                .filter(|v| (0.0..=1.0).contains(v))
        };
        match words.first().copied()? {
            "none" if words.len() == 1 => Some(LossModel::None),
            "bernoulli" if words.len() == 2 => Some(LossModel::Bernoulli { p: num(1)? }),
            "gilbert" if words.len() == 5 => {
                let m = LossModel::Gilbert {
                    p_gb: num(1)?,
                    p_bg: num(2)?,
                    loss_good: num(3)?,
                    loss_bad: num(4)?,
                };
                match m {
                    LossModel::Gilbert { p_gb, p_bg, .. } if p_gb > 0.0 && p_bg > 0.0 => Some(m),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// One UDP leg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leg {
    /// Fixed delay: send, the switch hops, receive, in us.
    pub base_us: u64,
    /// The jitter on top of it.
    pub jitter: JitterModel,
    /// How it loses datagrams.
    pub loss: LossModel,
}

/// One scenario, as read from a `config/sim-lowlat/*.lowlat` file.
#[derive(Debug, Clone, PartialEq)]
pub struct LowLatScenario {
    /// Name, from the file.
    pub name: String,
    /// What the scenario is for, one line.
    pub purpose: String,
    /// Seeds every draw.
    pub seed: u64,
    /// Run length, in ms of server timeline.
    pub duration_ms: u64,
    /// The plan: chunk, FEC, `L_tv` and the budget's other figures.
    pub plan: Plan,
    /// The TV's own audio output against its picture, in us: positive is
    /// the audio leaving the TV after the picture is shown. ASSUMED per
    /// scenario until the owner's TVs are known.
    pub tv_audio_lag_us: u64,
    /// The hub to the server.
    pub up: Leg,
    /// The server to the endpoint.
    pub down: Leg,
}

/// Why a scenario file was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioFileError(pub String);

impl fmt::Display for ScenarioFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ScenarioFileError {}

const KEYS: [&str; 17] = [
    "name",
    "purpose",
    "seed",
    "duration_ms",
    "chunk_frames",
    "sample_rate_hz",
    "fec_k",
    "fec_depth",
    "l_tv_us",
    "tv_audio_lag_us",
    "up_base_us",
    "up_jitter",
    "up_loss",
    "down_base_us",
    "down_jitter",
    "down_loss",
    "defaults",
];

impl LowLatScenario {
    /// Parse a scenario file: `key = value` lines, `#` comments. Every key
    /// is required except `defaults`; the budget figures not named here are
    /// [`DEFAULTS`]'s. `defaults = yes` says the file's chunk, FEC and
    /// `L_tv` are the defaults', which a test holds it to.
    pub fn parse(text: &str) -> Result<LowLatScenario, ScenarioFileError> {
        let bad = |why: String| ScenarioFileError(why);
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
        let leg = |prefix: &str| -> Result<Leg, ScenarioFileError> {
            let jitter_text = get(&format!("{}_jitter", prefix))?;
            let mut words = jitter_text.split_whitespace();
            let name = words.next().unwrap_or("");
            let scale = words
                .next()
                .map(|w| w.parse::<f64>())
                .transpose()
                .ok()
                .flatten()
                .unwrap_or(0.0);
            let jitter = JitterModel::from_name(name, scale).ok_or_else(|| {
                bad(format!(
                    "{}_jitter {:?} is not a model",
                    prefix, jitter_text
                ))
            })?;
            let loss_text = get(&format!("{}_loss", prefix))?;
            let loss = LossModel::parse(loss_text)
                .ok_or_else(|| bad(format!("{}_loss {:?} is not a model", prefix, loss_text)))?;
            Ok(Leg {
                base_us: whole(&format!("{}_base_us", prefix))?,
                jitter,
                loss,
            })
        };
        let plan = Plan {
            sample_rate_hz: whole("sample_rate_hz")? as u32,
            chunk_frames: whole("chunk_frames")? as u32,
            fec_k: whole("fec_k")? as u8,
            fec_depth: whole("fec_depth")? as u8,
            l_tv_ns: whole("l_tv_us")? * 1_000,
            ..DEFAULTS
        };
        plan.fec()
            .map_err(|e| bad(format!("fec_k and fec_depth: {}", e)))?;
        if plan.chunk_ns() == 0 {
            return Err(bad("a chunk of no time".to_string()));
        }
        plan.check_latency(plan.l_tv_ns)
            .map_err(|e| bad(format!("l_tv_us: {}", e)))?;
        let scenario = LowLatScenario {
            name: get("name")?.to_string(),
            purpose: get("purpose")?.to_string(),
            seed: whole("seed")?,
            duration_ms: whole("duration_ms")?,
            plan,
            tv_audio_lag_us: whole("tv_audio_lag_us")?,
            up: leg("up")?,
            down: leg("down")?,
        };
        if let Ok(d) = get("defaults") {
            let same = scenario.plan.chunk_frames == DEFAULTS.chunk_frames
                && scenario.plan.sample_rate_hz == DEFAULTS.sample_rate_hz
                && scenario.plan.fec_k == DEFAULTS.fec_k
                && scenario.plan.fec_depth == DEFAULTS.fec_depth
                && scenario.plan.l_tv_ns == DEFAULTS.l_tv_ns;
            if d != "yes" || !same {
                return Err(bad(
                    "defaults = yes, and only when chunk, FEC and L_tv are the defaults'"
                        .to_string(),
                ));
            }
        }
        Ok(scenario)
    }

    /// Chunks in the run.
    pub fn chunks(&self) -> u64 {
        self.duration_ms * 1_000_000 / self.plan.chunk_ns()
    }

    /// Sound against picture, ITU-R BT.1359-1 sign (positive: sound leads),
    /// in ns: the TV's own audio lag, the S/PDIF receiver and `L_tv`, all
    /// delaying the sound behind a picture the TV shows at once.
    pub fn lip_sync_ns(&self) -> i64 {
        -((self.tv_audio_lag_us * 1_000 + self.plan.spdif_receiver_ns + self.plan.l_tv_ns) as i64)
    }

    /// Whether [`LowLatScenario::lip_sync_ns`] is inside the window.
    pub fn lip_sync_ok(&self) -> bool {
        (LIP_SYNC_WINDOW_NS.0..=LIP_SYNC_WINDOW_NS.1).contains(&self.lip_sync_ns())
    }
}

/// One leg's drawn fates, the same for the run with FEC and without.
#[derive(Debug, Clone)]
struct Fates {
    data_lost: Vec<bool>,
    data_delay_ns: Vec<u64>,
    // Indexed by the chunk that completes the group (its parity's moment).
    parity_lost: Vec<bool>,
    parity_delay_ns: Vec<u64>,
}

fn draw(leg: &Leg, chunks: u64, data_seed: u64, parity_seed: u64) -> Fates {
    let mut data = Rng::new(data_seed);
    let mut parity = Rng::new(parity_seed);
    let n = chunks as usize;
    let mut f = Fates {
        data_lost: Vec::with_capacity(n),
        data_delay_ns: Vec::with_capacity(n),
        parity_lost: Vec::with_capacity(n),
        parity_delay_ns: Vec::with_capacity(n),
    };
    let mut bad = false;
    for _ in 0..n {
        let p_loss = match leg.loss {
            LossModel::None => 0.0,
            LossModel::Bernoulli { p } => p,
            LossModel::Gilbert {
                p_gb,
                p_bg,
                loss_good,
                loss_bad,
            } => {
                // The state moves once per chunk, on its own draw.
                let u = data.next_f64();
                bad = if bad { u >= p_bg } else { u < p_gb };
                if bad {
                    loss_bad
                } else {
                    loss_good
                }
            }
        };
        let base = leg.base_us * 1_000;
        f.data_lost.push(data.next_f64() < p_loss);
        f.data_delay_ns
            .push(base + leg.jitter.sample_ns(&mut data).round() as u64);
        f.parity_lost.push(parity.next_f64() < p_loss);
        f.parity_delay_ns
            .push(base + leg.jitter.sample_ns(&mut parity).round() as u64);
    }
    f
}

/// What one leg's decoder saw.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LegCounts {
    /// Data datagrams the leg lost.
    pub data_lost: u64,
    /// Parity datagrams the leg lost.
    pub parity_lost: u64,
    /// Chunks the receiver's FEC rebuilt.
    pub recovered: u64,
    /// Chunks the receiver's FEC could not rebuild.
    pub unrecoverable: u64,
    /// The longest run of consecutive chunks whose data datagram was lost.
    pub longest_burst: u64,
    /// The largest one-way delay of a delivered datagram, in ns.
    pub max_delay_ns: u64,
}

/// One run of one scenario.
#[derive(Debug, Clone, PartialEq)]
pub struct LowLatRun {
    /// The FEC this run used.
    pub fec_k: u8,
    /// Its interleave depth.
    pub fec_depth: u8,
    /// Chunks captured.
    pub chunks: u64,
    /// The hub's leg.
    pub up: LegCounts,
    /// The endpoint's leg.
    pub down: LegCounts,
    /// Chunks the endpoint had in time.
    pub on_time: u64,
    /// Chunks that arrived after their playout point and were dropped.
    pub late: u64,
    /// Chunks that never arrived.
    pub lost: u64,
    /// Chunks the endpoint had that were rebuilt on either leg.
    pub repaired: u64,
    /// The latest a received (never rebuilt) chunk was ready at the
    /// endpoint, from its stamp, in ns.
    pub worst_received_ns: u64,
    /// The latest a rebuilt chunk was ready at the endpoint, from its stamp,
    /// in ns.
    pub worst_repaired_ns: u64,
    /// The least lead a chunk had at the endpoint (its deadline minus when it
    /// was ready), in ns; negative when one was late.
    pub min_lead_ns: i64,
    /// The median lead, in ns.
    pub median_lead_ns: i64,
}

impl LowLatRun {
    /// Chunks not played: late or lost.
    pub fn residual(&self) -> u64 {
        self.late + self.lost
    }

    /// [`LowLatRun::residual`] as a fraction of the chunks.
    pub fn residual_rate(&self) -> f64 {
        self.residual() as f64 / self.chunks.max(1) as f64
    }
}

/// A scenario's two runs: its own FEC, and none.
#[derive(Debug, Clone, PartialEq)]
pub struct LowLatResult {
    /// With the scenario's FEC.
    pub fec: LowLatRun,
    /// Without FEC, on the same draws: the negative control.
    pub control: LowLatRun,
}

/// A chunk payload as the hub sends it: an `audio_chunk` header (sequence,
/// the capture stamp, 48 kHz, stereo, s16) and one frame of PCM. The audio
/// is irrelevant to the FEC and to the timing, so one frame keeps the run
/// fast; the FEC's bytes and lengths are exercised by the shared vectors.
fn payload(n: u64, stamp_ns: u64) -> Vec<u8> {
    let mut p = Vec::with_capacity(36);
    p.extend_from_slice(&(n as u32).to_be_bytes());
    p.extend_from_slice(&stamp_ns.to_be_bytes());
    p.extend_from_slice(&48_000u32.to_be_bytes());
    p.push(2);
    p.push(1);
    p.extend_from_slice(&[0u8; 14]);
    p.extend_from_slice(&(n as u32).to_le_bytes());
    p
}

fn stamp_of(payload: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&payload[4..12]);
    u64::from_be_bytes(b)
}

fn restamp(payload: &mut [u8], play_at_ns: u64) {
    payload[4..12].copy_from_slice(&play_at_ns.to_be_bytes());
}

struct Arrival {
    // Until `deliver` runs: when it was sent, and its drawn delay.
    sent_ns: u64,
    delay_ns: u64,
    at_ns: u64,
    order: u64,
    kind: Kind,
    plaintext: Vec<u8>,
}

fn longest_burst(lost: &[bool]) -> u64 {
    let (mut best, mut run) = (0u64, 0u64);
    for &l in lost {
        run = if l { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// Send one datagram down a leg: lost, or an arrival.
fn send(
    fates: &Fates,
    kind: Kind,
    chunk: u64,
    at_ns: u64,
    plaintext: Vec<u8>,
    out: &mut Vec<Arrival>,
    counts: &mut LegCounts,
) {
    let i = chunk as usize;
    let (lost, delay) = match kind {
        Kind::Data => (fates.data_lost[i], fates.data_delay_ns[i]),
        Kind::Parity => (fates.parity_lost[i], fates.parity_delay_ns[i]),
    };
    if lost {
        match kind {
            Kind::Data => counts.data_lost += 1,
            Kind::Parity => counts.parity_lost += 1,
        }
        return;
    }
    out.push(Arrival {
        sent_ns: at_ns,
        delay_ns: delay,
        at_ns: 0,
        order: out.len() as u64,
        kind,
        plaintext,
    });
}

/// One path, first in first out: in send order, each datagram arrives its
/// drawn delay after it was sent, but never before the one sent ahead of it
/// (a switched LAN keeps one flow's order; the jitter is queueing, not a
/// second path). The decoders take reordering too, which the shared vectors
/// hold; the model does not make it.
fn deliver(v: &mut [Arrival], counts: &mut LegCounts) {
    v.sort_by_key(|a| (a.sent_ns, a.order));
    let mut last = 0u64;
    for a in v.iter_mut() {
        a.at_ns = (a.sent_ns + a.delay_ns).max(last);
        last = a.at_ns;
        counts.max_delay_ns = counts.max_delay_ns.max(a.at_ns - a.sent_ns);
    }
}

fn run_once(
    s: &LowLatScenario,
    params: FecParams,
    up: &Fates,
    down: &Fates,
) -> Result<LowLatRun, String> {
    let plan = &s.plan;
    let chunk_ns = plan.chunk_ns();
    let chunks = s.chunks();
    let mut up_counts = LegCounts {
        longest_burst: longest_burst(&up.data_lost),
        ..LegCounts::default()
    };
    let mut down_counts = LegCounts {
        longest_burst: longest_burst(&down.data_lost),
        ..LegCounts::default()
    };

    // 1 and 2: the hub numbers, sends, and the first leg delivers.
    let mut encoder = FecEncoder::new(params);
    let mut arrivals = Vec::new();
    for n in 0..chunks {
        let stamp = n * chunk_ns;
        let ready = stamp + chunk_ns + plan.capture_ns;
        let mut p = payload(n, stamp);
        for d in encoder.push_payload(&mut p).map_err(|e| e.to_string())? {
            send(
                up,
                d.kind,
                n,
                ready,
                d.plaintext,
                &mut arrivals,
                &mut up_counts,
            );
        }
    }
    deliver(&mut arrivals, &mut up_counts);

    // 3: the server decodes, restamps and relays down the second leg.
    let mut server = FecDecoder::new(params);
    let mut relay = FecRelay::new(params);
    let mut down_arrivals = Vec::new();
    for a in &arrivals {
        for mut d in server.push(a.kind, &a.plaintext) {
            let play_at = stamp_of(&d.payload) + plan.l_tv_ns;
            restamp(&mut d.payload, play_at);
            let at = a.at_ns + plan.relay_ns;
            for out in relay.push(&d.payload).map_err(|e| e.to_string())? {
                send(
                    down,
                    out.kind,
                    d.chunk_index,
                    at,
                    out.plaintext,
                    &mut down_arrivals,
                    &mut down_counts,
                );
            }
        }
    }
    server.finish(chunks);
    up_counts.recovered = server.stats().recovered;
    up_counts.unrecoverable = server.stats().unrecoverable;
    deliver(&mut down_arrivals, &mut down_counts);

    // 4: the endpoint decodes; each chunk is ready when it is handed on.
    let mut endpoint = FecDecoder::new(params);
    let mut ready: Vec<Option<(u64, bool)>> = vec![None; chunks as usize];
    for a in &down_arrivals {
        for d in endpoint.push(a.kind, &a.plaintext) {
            let slot = &mut ready[d.chunk_index as usize];
            if slot.is_none() {
                *slot = Some((a.at_ns, d.recovered));
            }
        }
    }
    endpoint.finish(chunks);
    down_counts.recovered = endpoint.stats().recovered;
    down_counts.unrecoverable = endpoint.stats().unrecoverable;
    // A chunk the server rebuilt reached the endpoint as an ordinary one;
    // the hub's lost data datagram says which.
    let rebuilt_up = &up.data_lost;

    let output_ns = plan.endpoint_output_ns + plan.dac_filter_ns;
    let mut run = LowLatRun {
        fec_k: params.k(),
        fec_depth: params.depth(),
        chunks,
        up: up_counts,
        down: down_counts,
        on_time: 0,
        late: 0,
        lost: 0,
        repaired: 0,
        worst_received_ns: 0,
        worst_repaired_ns: 0,
        min_lead_ns: i64::MAX,
        median_lead_ns: 0,
    };
    let mut leads = Vec::with_capacity(chunks as usize);
    for (n, r) in ready.iter().enumerate() {
        let stamp = n as u64 * chunk_ns;
        let Some((at, recovered_down)) = *r else {
            run.lost += 1;
            continue;
        };
        let deadline = stamp + plan.l_tv_ns - output_ns;
        let lead = deadline as i64 - at as i64;
        leads.push(lead);
        run.min_lead_ns = run.min_lead_ns.min(lead);
        let from_stamp = at - stamp;
        if recovered_down || rebuilt_up[n] {
            run.repaired += 1;
            run.worst_repaired_ns = run.worst_repaired_ns.max(from_stamp);
        } else {
            run.worst_received_ns = run.worst_received_ns.max(from_stamp);
        }
        if at <= deadline {
            run.on_time += 1;
        } else {
            run.late += 1;
        }
    }
    leads.sort_unstable();
    run.median_lead_ns = leads.get(leads.len() / 2).copied().unwrap_or(0);
    if leads.is_empty() {
        run.min_lead_ns = 0;
    }
    Ok(run)
}

/// Run a scenario: its own FEC, then none, on the same draws.
pub fn run_lowlat(s: &LowLatScenario) -> Result<LowLatResult, String> {
    let chunks = s.chunks();
    let up = draw(&s.up, chunks, s.seed ^ UP_DATA, s.seed ^ UP_PARITY);
    let down = draw(&s.down, chunks, s.seed ^ DOWN_DATA, s.seed ^ DOWN_PARITY);
    let params = s.plan.fec().map_err(|e| e.to_string())?;
    Ok(LowLatResult {
        fec: run_once(s, params, &up, &down)?,
        control: run_once(s, FecParams::none(), &up, &down)?,
    })
}

/// A run of `s` with its `L_tv` replaced, below the floor if asked: the
/// floor rule's own negative control (the scenario parser refuses such a
/// value, so this bypasses it on purpose).
pub fn run_with_latency(s: &LowLatScenario, l_tv_ns: u64) -> Result<LowLatRun, String> {
    let mut t = s.clone();
    t.plan.l_tv_ns = l_tv_ns;
    let chunks = t.chunks();
    let up = draw(&t.up, chunks, t.seed ^ UP_DATA, t.seed ^ UP_PARITY);
    let down = draw(&t.down, chunks, t.seed ^ DOWN_DATA, t.seed ^ DOWN_PARITY);
    let params = t.plan.fec().map_err(|e| e.to_string())?;
    run_once(&t, params, &up, &down)
}

/// The expected per-chunk residual loss of independent loss `p` on each of
/// two legs with one parity per `k` (no interleave needed): a chunk is lost
/// on a leg when its datagram is and so is one of the other `k` datagrams of
/// its group, `p (1 - (1 - p)^k)`; the two legs add, to first order.
pub fn bernoulli_expectation(p: f64, k: u8) -> f64 {
    if k == 0 {
        return 1.0 - (1.0 - p) * (1.0 - p);
    }
    2.0 * p * (1.0 - (1.0 - p).powi(i32::from(k)))
}

fn ms(ns: u64) -> String {
    format!("{:.3}", ns as f64 / 1e6)
}

fn ms_signed(ns: i64) -> String {
    format!("{:.3}", ns as f64 / 1e6)
}

fn rate(n: u64, of: u64) -> String {
    if n == 0 {
        "0".to_string()
    } else {
        format!("{} ({:.1e})", n, n as f64 / of.max(1) as f64)
    }
}

fn w(o: &mut String, s: &str) {
    o.push_str(s);
    o.push('\n');
}

/// The report, `docs/measurements/low-latency-budget-sim.md`. `build` and
/// `note` fill the two provenance lines; nothing else depends on them.
pub fn report(build: &str, note: &str, runs: &[(LowLatScenario, LowLatResult)]) -> String {
    let mut o = String::new();
    let d = &DEFAULTS;
    w(
        &mut o,
        "# Low-latency TV path, the latency budget and residual loss: simulation",
    );
    w(&mut o, "");
    w(&mut o, "Source: simulation");
    w(&mut o, &format!("Build measured: `{}`", build));
    w(&mut o, &format!("Build note: {}", note));
    w(&mut o, "Date: 2026-10-01");
    w(&mut o, "Generated by: `cargo run --release -p chorus-sync --bin chorus-sim-lowlat -- --build <sha> --note <text> --out docs/measurements/low-latency-budget-sim.md`");
    w(&mut o, "");
    w(&mut o, "**THIS IS A SIMULATION AND NOT TIMING EVIDENCE** (BRIEF.md section 3.1 rule 3). Every number below is what the chorus low-latency model does with the figures listed here; the loss rates, the delays, the capture buffering, the relay and the endpoint's output path are ASSUMED or cited from datasheets and the research, none was measured on the owner's network, hub, TV or speakers, and nobody watched a picture against it. It says the defaults in `crates/protocol/src/v2/lowlat.rs` (`DEFAULTS`) are self-consistent and what the FEC buys in the model; bench session S8 and a LAN loss histogram replace the ASSUMED figures.");
    w(&mut o, "");
    w(&mut o, "## What was run");
    w(&mut o, "");
    w(&mut o, "`crates/sync/src/lowlat_sim.rs`: the TV's audio is captured on the hub and stamped at its capture instant on the server timeline; each 2.5 ms chunk goes up a UDP leg to the server, which restamps it (play at the capture stamp plus L_tv) and relays it down a second UDP leg to a wired endpoint. Both legs run the real FEC (`chorus_protocol::v2::lowlat`: `FecEncoder` on the hub, `FecDecoder` and the group-aligned `FecRelay` on the server, `FecDecoder` on the endpoint) over datagrams each leg loses (none, Bernoulli, or Gilbert-Elliott bursts) and delays (a base plus an exponential jitter draw, first in first out on each leg: a switched LAN keeps one flow's order). A chunk the endpoint has by its play-at stamp minus its output path (buffer, DSP block, DAC filter) plays; a later one is dropped as late (never played past its playout point); one that never arrives is lost. Late plus lost is the residual loss, heard as a crossfade to silence over one chunk.");
    w(&mut o, "");
    w(&mut o, "Every scenario runs twice on the same draws: with its FEC, and without it (the negative control). Data datagrams' fates are drawn per chunk and parity datagrams' per group from separate seeded streams (`crates/sync/src/rng.rs`), and the burst state advances once per chunk, so both runs lose exactly the same chunks and differ only by the parity. Seeded, so a run repeats exactly; `crates/sync/tests/lowlat_budget.rs` regenerates this report and fails on any difference but the two build lines.");
    w(&mut o, "");
    w(
        &mut o,
        "## The budget, from the capture stamp to the speaker (the defaults)",
    );
    w(&mut o, "");
    w(&mut o, "| stage | ms | source | basis |");
    w(&mut o, "|---|---|---|---|");
    for item in d.budget() {
        w(
            &mut o,
            &format!(
                "| {} | {} | {} | {} |",
                item.stage,
                ms(item.ns),
                item.source.label(),
                item.source.text()
            ),
        );
    }
    w(&mut o, &format!(
        "| **floor** (the sum: the least L_tv under which the worst-placed repaired chunk is in time) | **{}** | computed | `Plan::floor_ns`; an L_tv below it is refused, never raised |",
        ms(d.floor_ns())
    ));
    w(&mut o, &format!(
        "| **L_tv, the default** (configurable {} to {}) | **{}** | ASSUMED | the research's proposal (fec-latency.md section 6), {} ms over the floor |",
        ms(d.l_tv_range_ns.0),
        ms(d.l_tv_range_ns.1),
        ms(d.l_tv_ns),
        ms(d.l_tv_ns - d.floor_ns())
    ));
    w(&mut o, "");
    w(
        &mut o,
        "Before the stamp, so outside L_tv but counted against lip sync:",
    );
    w(&mut o, "");
    w(&mut o, "| stage | ms | source | basis |");
    w(&mut o, "|---|---|---|---|");
    w(&mut o, &format!(
        "| S/PDIF receiver (TI DIR9001, 3/fS) | {} | cited | https://www.ti.com/lit/ds/symlink/dir9001.pdf (read 2026-10-01) |",
        ms(d.spdif_receiver_ns)
    ));
    w(&mut o, &format!(
        "| the TV's own audio output against its picture | per scenario | ASSUMED | Needs item \"{}\"; standard mode on the one set measured: audio about 1 ms after the picture; game mode about 66 ms (https://avlatency.com/measuring-latency/measurement-examples/, read 2026-10-01, LEAD: one set) |",
        TV_NEEDS_ITEM
    ));
    w(&mut o, "");
    w(&mut o, "The lip-sync figure is sound against picture in ITU-R BT.1359-1's sign (positive: sound leads): -(TV audio lag + S/PDIF receiver + L_tv), judged against BRIEF.md section 2.2's window [-40, +15] ms. chorus can only delay audio, so a TV whose audio already lags its picture by more than the window minus L_tv cannot be saved by any setting (the A/V trim delays, never advances).");
    w(&mut o, "");
    w(&mut o, "## Summary");
    w(&mut o, "");
    w(&mut o, "| scenario | FEC k x depth | L_tv ms | floor ms | chunks | loss model (each leg) | data lost up / down | residual with FEC | residual without FEC | worst ready, received / repaired, ms from stamp | least lead ms | lip sync ms | verdict |");
    w(
        &mut o,
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|",
    );
    for (s, r) in runs {
        let f = &r.fec;
        w(
            &mut o,
            &format!(
                "| `{}` | {} | {} | {} | {} | {} | {} / {} | {} | {} | {} / {} | {} | {} | {} |",
                s.name,
                if s.plan.fec_k == 0 {
                    "none".to_string()
                } else {
                    format!("{} x {}", s.plan.fec_k, s.plan.fec_depth)
                },
                ms(s.plan.l_tv_ns),
                ms(s.plan.floor_ns()),
                f.chunks,
                s.up.loss.describe(),
                f.up.data_lost,
                f.down.data_lost,
                rate(f.residual(), f.chunks),
                rate(r.control.residual(), r.control.chunks),
                ms(f.worst_received_ns),
                if f.repaired == 0 {
                    "none".to_string()
                } else {
                    ms(f.worst_repaired_ns)
                },
                ms_signed(f.min_lead_ns),
                ms_signed(s.lip_sync_ns()),
                if s.lip_sync_ok() { "PASS" } else { "FAIL" }
            ),
        );
    }
    w(&mut o, "");
    for (s, r) in runs {
        w(&mut o, &format!("## Scenario `{}`", s.name));
        w(&mut o, "");
        w(
            &mut o,
            &format!(
            "`config/sim-lowlat/{}.lowlat`: {} Seed {}; {} s; {} Hz, {} frames a chunk ({} ms).",
            s.name,
            s.purpose,
            s.seed,
            s.duration_ms / 1000,
            s.plan.sample_rate_hz,
            s.plan.chunk_frames,
            ms(s.plan.chunk_ns())
        ),
        );
        w(&mut o, "");
        w(&mut o, "| parameter | value | from |");
        w(&mut o, "|---|---|---|");
        w(
            &mut o,
            &format!(
                "| FEC | k {} depth {}; FEC wait {} ms | {} |",
                s.plan.fec_k,
                s.plan.fec_depth,
                ms(s.plan.fec_wait_ns()),
                if s.plan.fec_k == DEFAULTS.fec_k && s.plan.fec_depth == DEFAULTS.fec_depth {
                    "the defaults"
                } else {
                    "this scenario"
                }
            ),
        );
        w(
            &mut o,
            &format!(
                "| L_tv | {} ms (floor {} ms) | {} |",
                ms(s.plan.l_tv_ns),
                ms(s.plan.floor_ns()),
                if s.plan.l_tv_ns == DEFAULTS.l_tv_ns {
                    "the default"
                } else {
                    "this scenario, at least its floor"
                }
            ),
        );
        w(
            &mut o,
            &format!(
                "| TV audio against its picture | {} ms late | ASSUMED (Needs item \"{}\") |",
                ms(s.tv_audio_lag_us * 1_000),
                TV_NEEDS_ITEM
            ),
        );
        for (label, leg) in [("hub to server", &s.up), ("server to endpoint", &s.down)] {
            w(&mut o, &format!(
                "| leg {}: delay | {} us base + {} jitter, scale {} us | ASSUMED (a quiet switched wired LAN; no chorus measurement yet) |",
                label,
                leg.base_us,
                leg.jitter.name(),
                leg.jitter.scale_us()
            ));
            w(&mut o, &format!(
                "| leg {}: loss | {} (long-run rate {:.1e}) | ASSUMED (fec-latency.md section 2: 1000BASE-T's BER bound is LEAD, home-LAN loss unmeasured) |",
                label,
                leg.loss.describe(),
                leg.loss.rate()
            ));
        }
        w(&mut o, "");
        w(&mut o, "| | with FEC | without FEC |");
        w(&mut o, "|---|---|---|");
        let (f, c) = (&r.fec, &r.control);
        let row = |o: &mut String, name: &str, a: String, b: String| {
            o.push_str(&format!("| {} | {} | {} |\n", name, a, b));
        };
        let mut t = String::new();
        row(&mut t, "chunks", f.chunks.to_string(), c.chunks.to_string());
        row(
            &mut t,
            "hub to server: data lost / parity lost / longest data burst",
            format!(
                "{} / {} / {}",
                f.up.data_lost, f.up.parity_lost, f.up.longest_burst
            ),
            format!("{} / - / {}", c.up.data_lost, c.up.longest_burst),
        );
        row(
            &mut t,
            "server: rebuilt / unrecoverable",
            format!("{} / {}", f.up.recovered, f.up.unrecoverable),
            format!("- / {}", c.up.unrecoverable),
        );
        row(
            &mut t,
            "server to endpoint: data lost / parity lost / longest data burst",
            format!(
                "{} / {} / {}",
                f.down.data_lost, f.down.parity_lost, f.down.longest_burst
            ),
            format!("{} / - / {}", c.down.data_lost, c.down.longest_burst),
        );
        row(
            &mut t,
            "endpoint: rebuilt / unrecoverable (counting chunks the server never had)",
            format!("{} / {}", f.down.recovered, f.down.unrecoverable),
            format!("- / {}", c.down.unrecoverable),
        );
        row(
            &mut t,
            "played on time",
            f.on_time.to_string(),
            c.on_time.to_string(),
        );
        row(
            &mut t,
            "late (dropped)",
            f.late.to_string(),
            c.late.to_string(),
        );
        row(&mut t, "lost", f.lost.to_string(), c.lost.to_string());
        row(
            &mut t,
            "**residual loss**",
            format!("**{}**", rate(f.residual(), f.chunks)),
            format!("**{}**", rate(c.residual(), c.chunks)),
        );
        row(
            &mut t,
            "worst ready from stamp: received / repaired (ms)",
            format!(
                "{} / {}",
                ms(f.worst_received_ns),
                if f.repaired == 0 {
                    "none".to_string()
                } else {
                    ms(f.worst_repaired_ns)
                }
            ),
            format!("{} / none", ms(c.worst_received_ns)),
        );
        row(
            &mut t,
            "lead at the endpoint, least / median (ms; the jitter buffer's hold before the output path)",
            format!(
                "{} / {}",
                ms_signed(f.min_lead_ns),
                ms_signed(f.median_lead_ns)
            ),
            format!(
                "{} / {}",
                ms_signed(c.min_lead_ns),
                ms_signed(c.median_lead_ns)
            ),
        );
        row(
            &mut t,
            "largest one-way delay, up / down (ms)",
            format!("{} / {}", ms(f.up.max_delay_ns), ms(f.down.max_delay_ns)),
            format!("{} / {}", ms(c.up.max_delay_ns), ms(c.down.max_delay_ns)),
        );
        o.push_str(&t);
        if let LossModel::Bernoulli { p } = s.up.loss {
            if s.up.loss == s.down.loss {
                w(&mut o, "");
                w(&mut o, &format!(
                    "Expected residual for independent loss {} on both legs (computed: 2 p (1 - (1 - p)^k) with FEC, 1 - (1 - p)^2 without): {:.1e} with FEC ({:.1} chunks of {}), {:.1e} without ({:.1} chunks).",
                    p,
                    bernoulli_expectation(p, s.plan.fec_k),
                    bernoulli_expectation(p, s.plan.fec_k) * f.chunks as f64,
                    f.chunks,
                    bernoulli_expectation(p, 0),
                    bernoulli_expectation(p, 0) * c.chunks as f64
                ));
            }
        }
        w(&mut o, "");
        w(
            &mut o,
            &format!(
                "Lip sync: -({} + {} + {}) = {} ms against [-40, +15]: **{}**.",
                ms(s.tv_audio_lag_us * 1_000),
                ms(s.plan.spdif_receiver_ns),
                ms(s.plan.l_tv_ns),
                ms_signed(s.lip_sync_ns()),
                if s.lip_sync_ok() { "PASS" } else { "FAIL" }
            ),
        );
        w(&mut o, "");
    }
    w(&mut o, "## What the model leaves out");
    w(&mut o, "");
    w(&mut o, "- The capture clock's drift against the timeline (the capture track's rate matching), the endpoint's sync error (the house simulation's), and the hosts' scheduling beyond the ASSUMED relay time: each adds to the jitter margin's job, none is modelled here.");
    w(&mut o, "- A link flap or outage longer than a block: FEC cannot cover it by design; the endpoint crossfades to silence and the counts say how long.");
    w(
        &mut o,
        "- Wi-Fi: the path is wired only (BRIEF.md 5.7); a wireless endpoint refuses the offer.",
    );
    w(&mut o, "- The AEAD: sealing and opening change no timing in the model and are held by the shared vectors (`fixtures/protocol/lowlat`), not here.");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loss_model_reads_and_refuses_as_written() {
        assert_eq!(LossModel::parse("none"), Some(LossModel::None));
        assert_eq!(
            LossModel::parse("bernoulli 0.001"),
            Some(LossModel::Bernoulli { p: 0.001 })
        );
        assert!(LossModel::parse("bernoulli 2").is_none());
        assert!(LossModel::parse("gilbert 0 0.5 0 1").is_none());
        let g = LossModel::parse("gilbert 0.0005 0.5 0 1").unwrap();
        assert!((g.rate() - 0.0005 / 0.5005).abs() < 1e-12);
    }

    #[test]
    fn a_burst_model_makes_bursts_and_a_bernoulli_one_mostly_does_not() {
        let leg = |loss| Leg {
            base_us: 80,
            jitter: JitterModel::None,
            loss,
        };
        let g = draw(
            &leg(LossModel::Gilbert {
                p_gb: 0.001,
                p_bg: 0.25,
                loss_good: 0.0,
                loss_bad: 1.0,
            }),
            200_000,
            1,
            2,
        );
        assert!(longest_burst(&g.data_lost) >= 8);
        let b = draw(&leg(LossModel::Bernoulli { p: 0.001 }), 200_000, 1, 2);
        assert!(longest_burst(&b.data_lost) <= 3);
    }
}
