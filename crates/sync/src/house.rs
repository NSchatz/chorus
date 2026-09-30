//! The simulator at house scale: many endpoints, one server timeline.
//!
//! [`crate::sim`] models one client against one server. A house is that, many
//! times over, against the SAME server: every endpoint has its own crystal,
//! its own network path, its own jitter stream and its own servo, and every
//! one of them is chasing the one timeline the server's clock defines. The
//! server is passive in the model (it only timestamps), so the endpoints do
//! not interact and each is one [`crate::sim::run`]; what the house adds is the
//! layout (rooms, bonded sets, wired or Wi-Fi) and the quantity BRIEF.md
//! section 2.2 is written in: inter-device error.
//!
//! Inter-device error for a pair of endpoints at a true time is the
//! difference of their modelled playout errors at that time. Both errors are
//! against the same server timeline, so the difference is how far apart the
//! two would be heard, ignoring everything this model does not have (DAC and
//! amplifier delay, acoustic path; see
//! `docs/decisions/0006-sync-simulator-and-servo.md`).
//!
//! A house is a text file under `config/sim-house/`, not `fixtures/sync/`:
//! the endpoint's C core does not read it (it models one endpoint, not a
//! house), and every file under `fixtures/sync/` is read by both. Why is in
//! the decision record that added this module.
//!
//! Everything here is a simulation. None of it is timing evidence (BRIEF.md
//! section 3.1 rule 3).

use std::fmt;

use crate::config::{ConfigError, SimConfig};
use crate::jitter::JitterModel;
use crate::rng::Rng;
use crate::servo::{ServoAction, ServoConfig};
use crate::sim::{run_recorded, SimResult};

/// BRIEF.md section 2.2, "Stereo pair / same room", acceptable, in ns.
pub const SAME_ROOM_ACCEPTABLE_NS: i64 = 500_000;
/// BRIEF.md section 2.2, "Stereo pair / same room", aspirational, in ns.
pub const SAME_ROOM_ASPIRATIONAL_NS: i64 = 200_000;
/// BRIEF.md section 2.2, "Multiroom music, different rooms", acceptable, in ns.
pub const CROSS_ROOM_ACCEPTABLE_NS: i64 = 5_000_000;
/// BRIEF.md section 2.2, "Multiroom music, different rooms", aspirational, in ns.
pub const CROSS_ROOM_ASPIRATIONAL_NS: i64 = 1_000_000;

/// Largest initial epoch offset an endpoint is given, in ns, either sign.
///
/// Two monotonic clocks have no shared origin, so any value would do; this
/// one is well past the hard resync threshold, so every endpoint has to
/// acquire the timeline rather than start on it.
pub const MAX_INITIAL_OFFSET_NS: f64 = 50_000_000.0;

/// How an endpoint reaches the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// Copper through the switch (and, in a routed house, the firewall).
    Wired,
    /// The house Wi-Fi.
    Wireless,
}

impl Transport {
    /// Stable name, as it appears in a house file.
    pub fn name(self) -> &'static str {
        match self {
            Transport::Wired => "wired",
            Transport::Wireless => "wireless",
        }
    }
}

/// A network path profile that endpoints refer to by name.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// The name endpoints use.
    pub name: String,
    /// Wired or wireless.
    pub transport: Transport,
    /// One-way base delay, in microseconds.
    pub base_one_way_delay_us: f64,
    /// Return minus forward base delay, in microseconds, before an
    /// endpoint's own trim.
    pub path_asymmetry_us: f64,
    /// Delay above the base.
    pub jitter: JitterModel,
}

/// How the endpoints of one room play together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomSet {
    /// One endpoint alone in its room.
    Single,
    /// Exactly two endpoints, bonded left and right.
    Stereo,
    /// Three or more endpoints, bonded as a theater set (with a sub).
    Theater,
}

impl RoomSet {
    fn from_name(name: &str) -> Option<RoomSet> {
        match name {
            "single" => Some(RoomSet::Single),
            "stereo" => Some(RoomSet::Stereo),
            "theater" => Some(RoomSet::Theater),
            _ => None,
        }
    }

    /// Stable name, as it appears in a house file.
    pub fn name(self) -> &'static str {
        match self {
            RoomSet::Single => "single",
            RoomSet::Stereo => "stereo",
            RoomSet::Theater => "theater",
        }
    }
}

/// One endpoint in the house.
#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    /// Unique name.
    pub id: String,
    /// The room it is in.
    pub room: String,
    /// How that room's endpoints are bonded.
    pub set: RoomSet,
    /// The link profile it uses.
    pub link: String,
    /// Its crystal error, in ppm.
    pub client_ppm: f64,
    /// Its crystal's random walk, in ppm per square root of a second.
    pub wander_ppm_per_sqrt_s: f64,
    /// Added to the link's asymmetry: this endpoint's own cable, PHY and
    /// stack, in microseconds.
    pub asymmetry_trim_us: f64,
}

/// A whole house, as read from a house file.
#[derive(Debug, Clone, PartialEq)]
pub struct HouseConfig {
    /// Name, from the file.
    pub name: String,
    /// Which network layout this file models (for the report).
    pub network: String,
    /// Seeds every endpoint's streams.
    pub seed: u64,
    /// Run length, in milliseconds of true time.
    pub duration_ms: u64,
    /// Sampling step, in milliseconds.
    pub step_ms: u64,
    /// Exchange cadence, in milliseconds.
    pub sync_interval_ms: u64,
    /// The part of the run excluded from every statistic, in milliseconds.
    pub settle_ms: u64,
    /// The server's crystal error, in ppm.
    pub server_ppm: f64,
    /// The correction law every endpoint runs.
    pub servo: ServoConfig,
    /// Link profiles, in file order.
    pub links: Vec<Link>,
    /// Endpoints, in file order.
    pub endpoints: Vec<Endpoint>,
}

/// Why a house file could not be used.
#[derive(Debug, Clone, PartialEq)]
pub enum HouseError {
    /// A line is not `key = value`, or a record has the wrong fields.
    Malformed {
        /// 1-based line number.
        line: usize,
        /// What is wrong.
        why: String,
    },
    /// A required key is absent, or a key appears twice.
    Key {
        /// The key.
        key: String,
        /// What is wrong with it.
        why: &'static str,
    },
    /// The layout breaks a rule (see [`HouseConfig::validate`]).
    Layout(String),
    /// One endpoint's configuration is refused by the simulator.
    Endpoint {
        /// The endpoint.
        id: String,
        /// Why.
        error: ConfigError,
    },
}

impl fmt::Display for HouseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HouseError::Malformed { line, why } => write!(f, "line {}: {}", line, why),
            HouseError::Key { key, why } => write!(f, "{}: {}", key, why),
            HouseError::Layout(why) => write!(f, "layout: {}", why),
            HouseError::Endpoint { id, error } => write!(f, "endpoint {}: {}", id, error),
        }
    }
}

impl std::error::Error for HouseError {}

const SCALAR_KEYS: [&str; 15] = [
    "name",
    "network",
    "seed",
    "duration_ms",
    "step_ms",
    "sync_interval_ms",
    "settle_ms",
    "server_ppm",
    "kp",
    "ki",
    "max_correction_ppm",
    "hard_resync_threshold_us",
    "filter_window",
    "smoothing_alpha",
    "wander_ppm_per_sqrt_s",
];

impl HouseConfig {
    /// Parse a house file.
    ///
    /// Scalar keys are `key = value`. A link is
    /// `link.<name> = <wired|wireless> <base_us> <asymmetry_us> <model> <scale_us>`
    /// with, for `burst`, `<enter_prob> <exit_prob> <burst_scale_us> <burst_shape>`
    /// appended. An endpoint is
    /// `endpoint.<id> = <room> <single|stereo|theater> <link> <client_ppm> <asymmetry_trim_us>`.
    /// `wander_ppm_per_sqrt_s` is one value for every crystal in the house.
    pub fn parse(text: &str) -> Result<HouseConfig, HouseError> {
        let mut scalars: Vec<(String, String)> = Vec::new();
        let mut links = Vec::new();
        let mut raw_endpoints: Vec<(usize, String, Vec<String>)> = Vec::new();

        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or_else(|| HouseError::Malformed {
                line: line_no,
                why: format!("{:?} is not `key = value`", line),
            })?;
            let key = key.trim();
            let value = value.trim();
            let fields: Vec<String> = value.split_whitespace().map(str::to_string).collect();
            if let Some(name) = key.strip_prefix("link.") {
                links.push(parse_link(line_no, name, &fields)?);
            } else if let Some(id) = key.strip_prefix("endpoint.") {
                raw_endpoints.push((line_no, id.to_string(), fields));
            } else if SCALAR_KEYS.contains(&key) {
                if scalars.iter().any(|(k, _)| k == key) {
                    return Err(HouseError::Key {
                        key: key.to_string(),
                        why: "appears twice",
                    });
                }
                scalars.push((key.to_string(), value.to_string()));
            } else {
                return Err(HouseError::Malformed {
                    line: line_no,
                    why: format!("unknown key {:?}", key),
                });
            }
        }

        let get = |key: &'static str| -> Result<&str, HouseError> {
            scalars
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .ok_or(HouseError::Key {
                    key: key.to_string(),
                    why: "is required",
                })
        };
        let num = |key: &'static str| -> Result<f64, HouseError> {
            get(key)?
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or(HouseError::Key {
                    key: key.to_string(),
                    why: "is not a finite number",
                })
        };
        let whole = |key: &'static str| -> Result<u64, HouseError> {
            get(key)?.parse::<u64>().map_err(|_| HouseError::Key {
                key: key.to_string(),
                why: "is not an unsigned whole number",
            })
        };

        let wander = num("wander_ppm_per_sqrt_s")?;
        let mut endpoints = Vec::new();
        for (line_no, id, fields) in raw_endpoints {
            if fields.len() != 5 {
                return Err(HouseError::Malformed {
                    line: line_no,
                    why: format!("endpoint.{} needs 5 fields, has {}", id, fields.len()),
                });
            }
            let set = RoomSet::from_name(&fields[1]).ok_or_else(|| HouseError::Malformed {
                line: line_no,
                why: format!("{:?} is not single, stereo or theater", fields[1]),
            })?;
            endpoints.push(Endpoint {
                id,
                room: fields[0].clone(),
                set,
                link: fields[2].clone(),
                client_ppm: field_f64(line_no, &fields[3])?,
                wander_ppm_per_sqrt_s: wander,
                asymmetry_trim_us: field_f64(line_no, &fields[4])?,
            });
        }

        let house = HouseConfig {
            name: get("name")?.to_string(),
            network: get("network")?.to_string(),
            seed: whole("seed")?,
            duration_ms: whole("duration_ms")?,
            step_ms: whole("step_ms")?,
            sync_interval_ms: whole("sync_interval_ms")?,
            settle_ms: whole("settle_ms")?,
            server_ppm: num("server_ppm")?,
            servo: ServoConfig {
                kp: num("kp")?,
                ki: num("ki")?,
                max_correction_ppm: num("max_correction_ppm")?,
                hard_resync_threshold_ns: num("hard_resync_threshold_us")? * 1_000.0,
                filter_window: whole("filter_window")? as usize,
                smoothing_alpha: num("smoothing_alpha")?,
            },
            links,
            endpoints,
        };
        house.validate()?;
        Ok(house)
    }

    /// Check the layout rules, then every endpoint's configuration.
    ///
    /// Endpoint ids and link names are unique; every endpoint names a link
    /// that exists; a room's endpoints all declare the same set; a stereo
    /// room holds exactly two, a theater room at least three and a single
    /// room one; a wireless endpoint is alone in its room (K91: Wi-Fi
    /// speakers are never bonded); the settle period leaves most of the run.
    pub fn validate(&self) -> Result<(), HouseError> {
        let layout = |why: String| Err(HouseError::Layout(why));
        for (i, link) in self.links.iter().enumerate() {
            if self.links[..i].iter().any(|l| l.name == link.name) {
                return layout(format!("link {} is defined twice", link.name));
            }
        }
        if self.endpoints.len() < 2 {
            return layout("a house needs at least two endpoints to have a pair".to_string());
        }
        for (i, endpoint) in self.endpoints.iter().enumerate() {
            if self.endpoints[..i].iter().any(|e| e.id == endpoint.id) {
                return layout(format!("endpoint {} is defined twice", endpoint.id));
            }
            let link = match self.link(&endpoint.link) {
                Some(link) => link,
                None => {
                    return layout(format!(
                        "endpoint {} names link {}, which is not defined",
                        endpoint.id, endpoint.link
                    ))
                }
            };
            let room: Vec<&Endpoint> = self
                .endpoints
                .iter()
                .filter(|e| e.room == endpoint.room)
                .collect();
            if room.iter().any(|e| e.set != endpoint.set) {
                return layout(format!("room {} mixes sets", endpoint.room));
            }
            let fits = match endpoint.set {
                RoomSet::Single => room.len() == 1,
                RoomSet::Stereo => room.len() == 2,
                RoomSet::Theater => room.len() >= 3,
            };
            if !fits {
                return layout(format!(
                    "room {} is declared {} and holds {} endpoints",
                    endpoint.room,
                    endpoint.set.name(),
                    room.len()
                ));
            }
            if link.transport == Transport::Wireless && room.len() != 1 {
                return layout(format!(
                    "{} is wireless and shares room {}; Wi-Fi speakers are never bonded (K91)",
                    endpoint.id, endpoint.room
                ));
            }
        }
        if self.settle_ms.saturating_mul(2) > self.duration_ms {
            return layout(format!(
                "a settle of {} ms leaves less than half of a {} ms run",
                self.settle_ms, self.duration_ms
            ));
        }
        for index in 0..self.endpoints.len() {
            let config = self.endpoint_config(index);
            config.validate().map_err(|error| HouseError::Endpoint {
                id: self.endpoints[index].id.clone(),
                error,
            })?;
        }
        Ok(())
    }

    /// The link profile with this name.
    pub fn link(&self, name: &str) -> Option<&Link> {
        self.links.iter().find(|l| l.name == name)
    }

    /// The transport of the endpoint at `index`.
    pub fn transport(&self, index: usize) -> Transport {
        self.link(&self.endpoints[index].link)
            .map(|l| l.transport)
            .unwrap_or(Transport::Wired)
    }

    /// The seed of the endpoint at `index`: the house seed plus the index,
    /// through one SplitMix64 step, so neighbouring endpoints get unrelated
    /// streams.
    pub fn endpoint_seed(&self, index: usize) -> u64 {
        Rng::new(self.seed.wrapping_add(index as u64)).next_u64()
    }

    /// The single-client run the endpoint at `index` is.
    ///
    /// Its initial epoch offset is drawn uniformly from
    /// +/- [`MAX_INITIAL_OFFSET_NS`] out of a stream of its own, so the house
    /// file does not have to carry a number that means nothing.
    pub fn endpoint_config(&self, index: usize) -> SimConfig {
        let endpoint = &self.endpoints[index];
        let link = self
            .link(&endpoint.link)
            .expect("validate checked every endpoint names a link");
        let seed = self.endpoint_seed(index);
        let mut offsets = Rng::new(seed ^ 0x6F66_6673_6574_0000);
        let initial_offset_ns = ((2.0 * offsets.next_f64() - 1.0) * MAX_INITIAL_OFFSET_NS) as i64;
        SimConfig {
            seed,
            duration_ms: self.duration_ms,
            step_ms: self.step_ms,
            sync_interval_ms: self.sync_interval_ms,
            server_ppm: self.server_ppm,
            client_ppm: endpoint.client_ppm,
            initial_offset_ns,
            base_one_way_delay_us: link.base_one_way_delay_us,
            path_asymmetry_us: link.path_asymmetry_us + endpoint.asymmetry_trim_us,
            jitter: link.jitter,
            client_wander_ppm_per_sqrt_s: endpoint.wander_ppm_per_sqrt_s,
            servo: self.servo,
        }
    }
}

fn field_f64(line: usize, value: &str) -> Result<f64, HouseError> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| HouseError::Malformed {
            line,
            why: format!("{:?} is not a finite number", value),
        })
}

fn parse_link(line: usize, name: &str, fields: &[String]) -> Result<Link, HouseError> {
    let bad = |why: String| HouseError::Malformed { line, why };
    if fields.len() < 5 {
        return Err(bad(format!("link.{} needs at least 5 fields", name)));
    }
    let transport = match fields[0].as_str() {
        "wired" => Transport::Wired,
        "wireless" => Transport::Wireless,
        other => return Err(bad(format!("{:?} is not wired or wireless", other))),
    };
    let base = field_f64(line, &fields[1])?;
    let asymmetry = field_f64(line, &fields[2])?;
    let scale = field_f64(line, &fields[4])?;
    let jitter = if fields[3] == "burst" {
        if fields.len() != 9 {
            return Err(bad(format!("a burst link.{} needs 9 fields", name)));
        }
        JitterModel::Burst {
            mean_us: scale,
            enter_prob: field_f64(line, &fields[5])?,
            exit_prob: field_f64(line, &fields[6])?,
            burst_scale_us: field_f64(line, &fields[7])?,
            burst_shape: field_f64(line, &fields[8])?,
        }
    } else {
        if fields.len() != 5 {
            return Err(bad(format!("link.{} needs 5 fields", name)));
        }
        JitterModel::from_name(&fields[3], scale)
            .ok_or_else(|| bad(format!("{:?} is not a jitter model", fields[3])))?
    };
    Ok(Link {
        name: name.to_string(),
        transport,
        base_one_way_delay_us: base,
        path_asymmetry_us: asymmetry,
        jitter,
    })
}

/// A class of endpoint pairs, graded against one BRIEF.md section 2.2 row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PairClass {
    /// Both in one stereo-bonded room.
    StereoPair,
    /// Both in one theater set.
    TheaterSet,
    /// Different rooms, both wired.
    CrossRoomWired,
    /// Different rooms, one wired and one wireless.
    CrossRoomMixed,
    /// Different rooms, both wireless.
    CrossRoomWireless,
}

impl PairClass {
    /// Every class, in report order.
    pub const ALL: [PairClass; 5] = [
        PairClass::StereoPair,
        PairClass::TheaterSet,
        PairClass::CrossRoomWired,
        PairClass::CrossRoomMixed,
        PairClass::CrossRoomWireless,
    ];

    /// The report's name for the class.
    pub fn label(self) -> &'static str {
        match self {
            PairClass::StereoPair => "same room: stereo pair (wired)",
            PairClass::TheaterSet => "same room: theater set (wired)",
            PairClass::CrossRoomWired => "cross-room: wired / wired",
            PairClass::CrossRoomMixed => "cross-room: wired / Wi-Fi",
            PairClass::CrossRoomWireless => "cross-room: Wi-Fi / Wi-Fi",
        }
    }

    /// BRIEF.md section 2.2's acceptable bound for the class, in ns.
    pub fn acceptable_ns(self) -> i64 {
        match self {
            PairClass::StereoPair | PairClass::TheaterSet => SAME_ROOM_ACCEPTABLE_NS,
            _ => CROSS_ROOM_ACCEPTABLE_NS,
        }
    }

    /// BRIEF.md section 2.2's aspirational bound for the class, in ns.
    pub fn aspirational_ns(self) -> i64 {
        match self {
            PairClass::StereoPair | PairClass::TheaterSet => SAME_ROOM_ASPIRATIONAL_NS,
            _ => CROSS_ROOM_ASPIRATIONAL_NS,
        }
    }
}

/// The distribution of |inter-device error| over one class, after settling.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassStats {
    /// The class.
    pub class: PairClass,
    /// Pairs in it.
    pub pairs: usize,
    /// Samples pooled (pairs times post-settle steps).
    pub samples: usize,
    /// Median |error|, nearest rank, in ns.
    pub p50_ns: i64,
    /// 95th percentile |error|, nearest rank, in ns.
    pub p95_ns: i64,
    /// Largest |error|, in ns.
    pub max_ns: i64,
}

impl ClassStats {
    /// Whether every sample is under the acceptable bound.
    pub fn within_acceptable(&self) -> bool {
        self.max_ns < self.class.acceptable_ns()
    }

    /// Whether every sample is under the aspirational bound.
    pub fn within_aspirational(&self) -> bool {
        self.max_ns < self.class.aspirational_ns()
    }

    /// The report's verdict.
    pub fn verdict(&self) -> &'static str {
        if !self.within_acceptable() {
            "FAIL"
        } else if self.within_aspirational() {
            "pass (also inside aspirational)"
        } else {
            "pass (acceptable; above aspirational)"
        }
    }
}

/// One endpoint's run inside the house.
#[derive(Debug, Clone, PartialEq)]
pub struct EndpointRun {
    /// The endpoint's id.
    pub id: String,
    /// The single-client result.
    pub result: SimResult,
    /// Hard resyncs at or after the settle time.
    pub hard_resyncs_after_settle: u32,
    /// Largest |playout error| against the server timeline after settling,
    /// in ns.
    pub peak_error_after_settle_ns: i64,
}

/// Everything a house run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct HouseResult {
    /// Per endpoint, in file order.
    pub endpoints: Vec<EndpointRun>,
    /// Index of the first post-settle sample.
    pub settle_index: usize,
    /// Per class, in [`PairClass::ALL`] order, classes with no pairs left out.
    pub classes: Vec<ClassStats>,
}

/// The class of the pair `(a, b)`, by index into the house's endpoints.
pub fn pair_class(house: &HouseConfig, a: usize, b: usize) -> PairClass {
    let (ea, eb) = (&house.endpoints[a], &house.endpoints[b]);
    if ea.room == eb.room {
        return match ea.set {
            RoomSet::Theater => PairClass::TheaterSet,
            _ => PairClass::StereoPair,
        };
    }
    match (house.transport(a), house.transport(b)) {
        (Transport::Wired, Transport::Wired) => PairClass::CrossRoomWired,
        (Transport::Wireless, Transport::Wireless) => PairClass::CrossRoomWireless,
        _ => PairClass::CrossRoomMixed,
    }
}

/// Nearest-rank percentile of `values`, which it reorders. `p` in (0, 1].
fn nearest_rank(values: &mut [i64], p: f64) -> i64 {
    let rank = ((p * values.len() as f64).ceil() as usize).clamp(1, values.len());
    *values.select_nth_unstable(rank - 1).1
}

/// Run the house.
pub fn run_house(house: &HouseConfig) -> Result<HouseResult, HouseError> {
    house.validate()?;
    let settle_index = (house.settle_ms / house.step_ms) as usize;

    let mut endpoints = Vec::with_capacity(house.endpoints.len());
    for index in 0..house.endpoints.len() {
        let config = house.endpoint_config(index);
        let (result, records) = run_recorded(&config).map_err(|error| HouseError::Endpoint {
            id: house.endpoints[index].id.clone(),
            error,
        })?;
        // Exchange k happens at true time (k + 1) intervals.
        let hard_resyncs_after_settle = records
            .iter()
            .filter(|r| matches!(r.action, ServoAction::HardResync { .. }))
            .filter(|r| (u64::from(r.index) + 1) * house.sync_interval_ms >= house.settle_ms)
            .count() as u32;
        let peak_error_after_settle_ns = result.max_abs_error_after(settle_index);
        endpoints.push(EndpointRun {
            id: house.endpoints[index].id.clone(),
            result,
            hard_resyncs_after_settle,
            peak_error_after_settle_ns,
        });
    }

    let mut classes = Vec::new();
    for class in PairClass::ALL {
        let mut pairs = 0usize;
        let mut values: Vec<i64> = Vec::new();
        for a in 0..endpoints.len() {
            for b in a + 1..endpoints.len() {
                if pair_class(house, a, b) != class {
                    continue;
                }
                pairs += 1;
                let (sa, sb) = (&endpoints[a].result.samples, &endpoints[b].result.samples);
                values.extend(
                    sa[settle_index..]
                        .iter()
                        .zip(&sb[settle_index..])
                        .map(|(x, y)| (x.error_ns - y.error_ns).saturating_abs()),
                );
            }
        }
        if pairs == 0 {
            continue;
        }
        let max_ns = values.iter().copied().max().unwrap_or(0);
        let p95_ns = nearest_rank(&mut values, 0.95);
        let p50_ns = nearest_rank(&mut values, 0.50);
        classes.push(ClassStats {
            class,
            pairs,
            samples: values.len(),
            p50_ns,
            p95_ns,
            max_ns,
        });
    }

    Ok(HouseResult {
        endpoints,
        settle_index,
        classes,
    })
}

#[cfg(test)]
mod tests {
    use super::{nearest_rank, pair_class, run_house, HouseConfig, HouseError, PairClass};

    const SMALL: &str = "\
name = small
network = test
seed = 1
duration_ms = 20000
step_ms = 10
sync_interval_ms = 500
settle_ms = 5000
server_ppm = 0.0
kp = 0.4
ki = 0.08
max_correction_ppm = 300
hard_resync_threshold_us = 2000
filter_window = 16
smoothing_alpha = 0.25
wander_ppm_per_sqrt_s = 0.0
link.wired = wired 150 0 uniform 60
link.wifi = wireless 1500 300 burst 400 0.03 0.2 3000 1.5
endpoint.a-l = a stereo wired 20 0
endpoint.a-r = a stereo wired -20 5
endpoint.b = b single wired 10 0
endpoint.c = c single wifi 30 0
";

    #[test]
    fn a_small_house_parses_and_classifies_its_pairs() {
        let house = HouseConfig::parse(SMALL).expect("parses");
        assert_eq!(house.endpoints.len(), 4);
        assert_eq!(pair_class(&house, 0, 1), PairClass::StereoPair);
        assert_eq!(pair_class(&house, 0, 2), PairClass::CrossRoomWired);
        assert_eq!(pair_class(&house, 0, 3), PairClass::CrossRoomMixed);
        assert_eq!(house.endpoint_config(1).path_asymmetry_us, 5.0);
        assert_ne!(house.endpoint_seed(0), house.endpoint_seed(1));
    }

    #[test]
    fn a_small_house_runs_and_counts_its_pairs() {
        let house = HouseConfig::parse(SMALL).expect("parses");
        let result = run_house(&house).expect("runs");
        let pairs: usize = result.classes.iter().map(|c| c.pairs).sum();
        assert_eq!(pairs, 6, "four endpoints make six pairs");
        assert_eq!(result.settle_index, 500);
        for class in &result.classes {
            assert_eq!(class.samples, class.pairs * 1_500);
            assert!(class.p50_ns <= class.p95_ns && class.p95_ns <= class.max_ns);
        }
    }

    #[test]
    fn a_bonded_wireless_speaker_is_refused() {
        let text = SMALL.replace(
            "endpoint.a-r = a stereo wired",
            "endpoint.a-r = a stereo wifi",
        );
        match HouseConfig::parse(&text) {
            Err(HouseError::Layout(why)) => assert!(why.contains("K91"), "{}", why),
            other => panic!("expected a layout refusal, got {:?}", other),
        }
    }

    #[test]
    fn a_stereo_room_of_three_is_refused() {
        let text = format!("{}endpoint.a-c = a stereo wired 0 0\n", SMALL);
        assert!(matches!(
            HouseConfig::parse(&text),
            Err(HouseError::Layout(_))
        ));
    }

    #[test]
    fn an_unknown_key_is_refused() {
        let text = format!("{}sede = 3\n", SMALL);
        assert!(matches!(
            HouseConfig::parse(&text),
            Err(HouseError::Malformed { .. })
        ));
    }

    #[test]
    fn nearest_rank_is_nearest_rank() {
        let mut v: Vec<i64> = (1..=100).collect();
        assert_eq!(nearest_rank(&mut v, 0.5), 50);
        assert_eq!(nearest_rank(&mut v, 0.95), 95);
        assert_eq!(nearest_rank(&mut v, 1.0), 100);
        let mut one = vec![7];
        assert_eq!(nearest_rank(&mut one, 0.5), 7);
    }
}
