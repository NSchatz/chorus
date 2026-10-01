//! What configures the chain: the room's [`SoundSettings`] (exactly the wire
//! `sound` message's fields, protocol v2 0x39) and the endpoint's own
//! [`EndpointDsp`] (its drivers: the two-way split and per-output delays,
//! which are the speaker's and not the catalog's).

use crate::DspError;

/// Channel positions, as `docs/protocol.md` "The channel map" numbers them
/// (the `WAVEFORMATEXTENSIBLE` bit order plus `MONO = 0`).
pub mod position {
    pub const MONO: u8 = 0;
    pub const FL: u8 = 1;
    pub const FR: u8 = 2;
    pub const FC: u8 = 3;
    pub const LFE: u8 = 4;
    /// The highest position the map defines (`TBR`).
    pub const MAX: u8 = 18;
}

/// The bass and treble range, dB (one step is 1 dB; ASSUMED, the catalog's).
pub const TONE_MIN_DB: i8 = -10;
/// See [`TONE_MIN_DB`].
pub const TONE_MAX_DB: i8 = 10;
/// The bass-management crossover range, Hz.
pub const CROSSOVER_MIN_HZ: u16 = 40;
/// See [`CROSSOVER_MIN_HZ`].
pub const CROSSOVER_MAX_HZ: u16 = 200;
/// The default crossover: "The most common crossover frequency recommended
/// (and the THX standard) is 80 Hz" (SVS, "Tips for Setting the Crossover
/// Frequency of a Subwoofer",
/// <https://www.svsound.com/blogs/subwoofer-setup-and-tuning/tips-for-setting-the-proper-crossover-frequency-for-a-subwoofer>,
/// read 2026-10-01).
pub const CROSSOVER_DEFAULT_HZ: u16 = 80;
/// The subwoofer level range, centi-dB.
pub const SUB_LEVEL_MIN_CDB: i16 = -1200;
/// See [`SUB_LEVEL_MIN_CDB`].
pub const SUB_LEVEL_MAX_CDB: i16 = 600;
/// The room-correction bounds, the wire's and the catalog's (`crates/control`
/// owns the catalog's `ROOM_EQ_*`; these are the same numbers, held here so
/// the chain refuses what the wire would): at most 8 filters,
pub const ROOM_EQ_MAX_FILTERS: usize = 8;
/// 20..=1000 Hz,
pub const ROOM_EQ_FREQ_MIN_HZ: u16 = 20;
/// See [`ROOM_EQ_FREQ_MIN_HZ`].
pub const ROOM_EQ_FREQ_MAX_HZ: u16 = 1000;
/// -1200..=300 centi-dB,
pub const ROOM_EQ_GAIN_MIN_CDB: i16 = -1200;
/// See [`ROOM_EQ_GAIN_MIN_CDB`].
pub const ROOM_EQ_GAIN_MAX_CDB: i16 = 300;
/// and Q 0.5..=10.0 in thousandths.
pub const ROOM_EQ_Q_MIN_MILLI: u16 = 500;
/// See [`ROOM_EQ_Q_MIN_MILLI`].
pub const ROOM_EQ_Q_MAX_MILLI: u16 = 10000;

/// One room-correction peaking filter, in the wire's integer units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomEqFilter {
    pub freq_hz: u16,
    pub gain_cdb: i16,
    pub q_milli: u16,
}

/// The room's sound, as the wire `sound` message carries it to an endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundSettings {
    /// The low shelf, dB, -10..=10.
    pub bass_db: i8,
    /// The high shelf, dB, -10..=10.
    pub treble_db: i8,
    /// ISO 226 loudness compensation of the volume.
    pub loudness: bool,
    /// The night compressor.
    pub night: bool,
    /// Speech enhancement.
    pub speech: bool,
    /// Whether `room_eq` is applied.
    pub room_eq_enabled: bool,
    /// The subwoofer feed's polarity is inverted.
    pub sub_polarity_inverted: bool,
    /// The endpoint's channel position in the room's bonded set, 0 when it is
    /// not in one.
    pub role: u8,
    /// The set has an LFE member, so bass management is on.
    pub sub_present: bool,
    /// The bass-management crossover, Hz, 40..=200.
    pub crossover_hz: u16,
    /// The subwoofer level, centi-dB, -1200..=600.
    pub sub_level_cdb: i16,
    /// Up to 8 room-correction filters.
    pub room_eq: Vec<RoomEqFilter>,
}

impl Default for SoundSettings {
    /// Flat: every stage bypassed, not in a set, the default crossover.
    fn default() -> Self {
        SoundSettings {
            bass_db: 0,
            treble_db: 0,
            loudness: false,
            night: false,
            speech: false,
            room_eq_enabled: false,
            sub_polarity_inverted: false,
            role: 0,
            sub_present: false,
            crossover_hz: CROSSOVER_DEFAULT_HZ,
            sub_level_cdb: 0,
            room_eq: Vec::new(),
        }
    }
}

impl SoundSettings {
    /// Refuses a field outside the wire `sound` message's bounds, by name.
    pub fn validate(&self) -> Result<(), DspError> {
        if !(TONE_MIN_DB..=TONE_MAX_DB).contains(&self.bass_db) {
            return Err(DspError::Setting("bass_db"));
        }
        if !(TONE_MIN_DB..=TONE_MAX_DB).contains(&self.treble_db) {
            return Err(DspError::Setting("treble_db"));
        }
        if self.role > position::MAX {
            return Err(DspError::Setting("role"));
        }
        if !(CROSSOVER_MIN_HZ..=CROSSOVER_MAX_HZ).contains(&self.crossover_hz) {
            return Err(DspError::Setting("crossover_hz"));
        }
        if !(SUB_LEVEL_MIN_CDB..=SUB_LEVEL_MAX_CDB).contains(&self.sub_level_cdb) {
            return Err(DspError::Setting("sub_level_cdb"));
        }
        if self.room_eq.len() > ROOM_EQ_MAX_FILTERS {
            return Err(DspError::Setting("eq_count"));
        }
        for f in &self.room_eq {
            if !(ROOM_EQ_FREQ_MIN_HZ..=ROOM_EQ_FREQ_MAX_HZ).contains(&f.freq_hz) {
                return Err(DspError::Setting("freq_hz"));
            }
            if !(ROOM_EQ_GAIN_MIN_CDB..=ROOM_EQ_GAIN_MAX_CDB).contains(&f.gain_cdb) {
                return Err(DspError::Setting("gain_cdb"));
            }
            if !(ROOM_EQ_Q_MIN_MILLI..=ROOM_EQ_Q_MAX_MILLI).contains(&f.q_milli) {
                return Err(DspError::Setting("q_milli"));
            }
        }
        Ok(())
    }
}

/// One driver of a two-way endpoint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Driver {
    /// The driver's trim, centi-dB, cut only (-2400..=0).
    pub trim_cdb: i16,
    /// The driver's alignment delay, us (added to its output's delay).
    pub delay_us: u32,
    /// The driver is wired (or wants to be driven) inverted.
    pub inverted: bool,
}

/// The lowest trim a driver takes, centi-dB. ASSUMED: -24 dB.
pub const DRIVER_TRIM_MIN_CDB: i16 = -2400;

/// The endpoint's two-way split: its input divided by LR4 at `crossover_hz`
/// into a woofer (output 0) and a tweeter (output 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TwoWay {
    pub crossover_hz: u32,
    pub woofer: Driver,
    pub tweeter: Driver,
}

/// The two-way example the design envelope gives until goals 24-25 design the
/// drivers: 2 kHz. ASSUMED.
pub const TWO_WAY_EXAMPLE_HZ: u32 = 2000;

/// The most outputs a chain has (the protocol's 8 channels).
pub const MAX_OUTPUTS: usize = 8;

/// The endpoint's own DSP: its drivers, not the room's sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndpointDsp {
    /// The two-way split, if the endpoint has one.
    pub two_way: Option<TwoWay>,
    /// A delay per output, us (the two-way drivers' own delays add to
    /// outputs 0 and 1).
    pub output_delay_us: [u32; MAX_OUTPUTS],
}

impl EndpointDsp {
    /// Refuses a field out of range, by name. The crossover must sit inside
    /// 20 Hz..0.45 x the rate (the corner rule the chain applies).
    pub fn validate(&self, rate_hz: u32) -> Result<(), DspError> {
        if let Some(t) = &self.two_way {
            if t.crossover_hz < 20 || t.crossover_hz as f64 > 0.45 * rate_hz as f64 {
                return Err(DspError::Endpoint("crossover_hz"));
            }
            for d in [&t.woofer, &t.tweeter] {
                if !(DRIVER_TRIM_MIN_CDB..=0).contains(&d.trim_cdb) {
                    return Err(DspError::Endpoint("trim_cdb"));
                }
            }
        }
        Ok(())
    }
}
