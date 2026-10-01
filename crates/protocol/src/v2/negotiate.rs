//! Codec negotiation: which codec, if any, a stream goes to an endpoint in.
//!
//! One step, on the server, from three inputs: the server's preference for
//! this endpoint (by link: decision K62), the codecs the server can actually
//! send today, and the endpoint's `capabilities`. The first preferred codec
//! that both sides can use and that can carry this stream wins. PCM is in
//! every endpoint's set, so a server that can send PCM always finds one; a
//! stream the endpoint cannot play at all (its rate, channel count or sample
//! format) is refused by name rather than sent in the hope that it copes.
//! `docs/protocol.md`, "Codecs: PCM, FLAC and Opus", and
//! `docs/decisions/0040-codec-negotiation.md`.

use std::fmt;

use crate::message::SampleFormat;
use crate::v2::catalog::{Codec, Link};
use crate::v2::messages::Capabilities;

/// The stream a server wants to send, before a codec is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    /// Sample rate of the audio.
    pub sample_rate_hz: u32,
    /// Channel count.
    pub channels: u8,
    /// PCM layout of the audio.
    pub sample_format: SampleFormat,
}

/// Why no codec could be agreed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The endpoint does not play this rate natively (the server does not
    /// resample yet).
    Rate {
        /// The stream's rate.
        wanted: u32,
        /// What the endpoint lists.
        offered: Vec<u32>,
    },
    /// The stream has more channels than the endpoint plays.
    Channels {
        /// The stream's channel count.
        wanted: u8,
        /// The endpoint's maximum.
        max: u8,
    },
    /// The endpoint does not play this sample format.
    SampleFormat {
        /// The stream's format.
        wanted: SampleFormat,
    },
    /// No preferred codec is one both sides can use for this stream.
    NoCommonCodec {
        /// The preference, in order.
        preferred: Vec<Codec>,
        /// The endpoint's codec bit set.
        endpoint: u8,
        /// What the server can send.
        server: Vec<Codec>,
    },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Rate { wanted, offered } => write!(
                f,
                "the endpoint does not play {} Hz (it lists {:?}) and the server does not resample yet",
                wanted, offered
            ),
            Refusal::Channels { wanted, max } => {
                write!(f, "the stream has {} channels and the endpoint plays at most {}", wanted, max)
            }
            Refusal::SampleFormat { wanted } => {
                write!(f, "the endpoint does not play {}", wanted.name())
            }
            Refusal::NoCommonCodec { preferred, endpoint, server } => write!(
                f,
                "no codec in the preference {:?} is one the endpoint (codec bits 0x{:02x}) and the server ({:?}) can both use for this stream",
                preferred.iter().map(|c| c.name()).collect::<Vec<_>>(),
                endpoint,
                server.iter().map(|c| c.name()).collect::<Vec<_>>()
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// The default preference for an endpoint on this link (decision K62):
/// lossless PCM first on a wired link, where bandwidth is not the constraint;
/// FLAC first on Wi-Fi, where a lossless codec spends less airtime than PCM
/// (how much less depends on the music and is not measured here); PCM last everywhere, as the codec every endpoint has. Opus is never a
/// default: it is lossy, and it is chosen per room by configuration where a
/// weak link or many rooms make lossless impossible.
pub fn default_preference(link: Link) -> Vec<Codec> {
    match link {
        Link::Wireless => vec![Codec::Flac, Codec::Pcm],
        Link::Wired | Link::Unknown => vec![Codec::Pcm],
    }
}

/// Whether a codec can carry this stream at all, whoever sends it.
fn carries(codec: Codec, source: &Source) -> bool {
    match codec {
        Codec::Pcm => true,
        // FLAC describes samples only by a bit depth of 4 to 32 (RFC 9639
        // section 8.2, read 2026-09-30); it has no floating-point format.
        Codec::Flac => source.sample_format != SampleFormat::PcmF32Le,
        // Opus decodes at 48 kHz (RFC 6716); the server does not resample yet.
        Codec::Opus => source.sample_rate_hz == 48_000,
    }
}

fn format_bit(format: SampleFormat) -> u8 {
    1 << (format.to_wire() - 1)
}

/// Choose the codec for `source` to an endpoint with `caps`.
pub fn negotiate(
    source: &Source,
    preference: &[Codec],
    server_can_send: &[Codec],
    caps: &Capabilities,
) -> Result<Codec, Refusal> {
    if !caps.sample_rates_hz.contains(&source.sample_rate_hz) {
        return Err(Refusal::Rate {
            wanted: source.sample_rate_hz,
            offered: caps.sample_rates_hz.clone(),
        });
    }
    if source.channels > caps.max_channels {
        return Err(Refusal::Channels {
            wanted: source.channels,
            max: caps.max_channels,
        });
    }
    if caps.sample_formats & format_bit(source.sample_format) == 0 {
        return Err(Refusal::SampleFormat {
            wanted: source.sample_format,
        });
    }
    preference
        .iter()
        .copied()
        .find(|c| caps.codecs & c.bit() != 0 && server_can_send.contains(c) && carries(*c, source))
        .ok_or_else(|| Refusal::NoCommonCodec {
            preferred: preference.to_vec(),
            endpoint: caps.codecs,
            server: server_can_send.to_vec(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(codecs: u8) -> Capabilities {
        Capabilities {
            codecs,
            sample_formats: 0b011,
            max_channels: 2,
            sample_rates_hz: vec![44_100, 48_000],
            buffer_ms: 500,
            intrinsic_latency_ns: 0,
            led_count: 0,
            visualizer_bands: 0,
            features: 0,
        }
    }

    const ALL: [Codec; 3] = [Codec::Pcm, Codec::Flac, Codec::Opus];
    const STEREO_48K: Source = Source {
        sample_rate_hz: 48_000,
        channels: 2,
        sample_format: SampleFormat::PcmS16Le,
    };

    #[test]
    fn the_first_preferred_codec_both_sides_have_wins() {
        let every = Codec::Pcm.bit() | Codec::Flac.bit() | Codec::Opus.bit();
        assert_eq!(
            negotiate(&STEREO_48K, &[Codec::Opus, Codec::Pcm], &ALL, &caps(every)),
            Ok(Codec::Opus)
        );
        assert_eq!(
            negotiate(
                &STEREO_48K,
                &default_preference(Link::Wireless),
                &ALL,
                &caps(every)
            ),
            Ok(Codec::Flac)
        );
        assert_eq!(
            negotiate(
                &STEREO_48K,
                &default_preference(Link::Wired),
                &ALL,
                &caps(every)
            ),
            Ok(Codec::Pcm)
        );
    }

    #[test]
    fn a_pcm_only_endpoint_on_wifi_falls_back_to_pcm() {
        assert_eq!(
            negotiate(
                &STEREO_48K,
                &default_preference(Link::Wireless),
                &ALL,
                &caps(Codec::Pcm.bit())
            ),
            Ok(Codec::Pcm)
        );
    }

    #[test]
    fn a_server_that_cannot_encode_flac_yet_sends_pcm() {
        let every = Codec::Pcm.bit() | Codec::Flac.bit();
        assert_eq!(
            negotiate(
                &STEREO_48K,
                &[Codec::Flac, Codec::Pcm],
                &[Codec::Pcm],
                &caps(every)
            ),
            Ok(Codec::Pcm)
        );
    }

    #[test]
    fn opus_is_skipped_off_48k_and_flac_is_skipped_for_float() {
        let every = Codec::Pcm.bit() | Codec::Flac.bit() | Codec::Opus.bit();
        let at_44k = Source {
            sample_rate_hz: 44_100,
            ..STEREO_48K
        };
        assert_eq!(
            negotiate(&at_44k, &[Codec::Opus, Codec::Pcm], &ALL, &caps(every)),
            Ok(Codec::Pcm)
        );
        let mut c = caps(every);
        c.sample_formats = 0b100;
        let float = Source {
            sample_format: SampleFormat::PcmF32Le,
            ..STEREO_48K
        };
        assert_eq!(
            negotiate(&float, &[Codec::Flac, Codec::Pcm], &ALL, &c),
            Ok(Codec::Pcm)
        );
    }

    #[test]
    fn a_stream_the_endpoint_cannot_play_is_refused_by_name() {
        let c = caps(Codec::Pcm.bit());
        let at_96k = Source {
            sample_rate_hz: 96_000,
            ..STEREO_48K
        };
        assert!(matches!(
            negotiate(&at_96k, &[Codec::Pcm], &ALL, &c),
            Err(Refusal::Rate { wanted: 96_000, .. })
        ));
        let six = Source {
            channels: 6,
            ..STEREO_48K
        };
        assert_eq!(
            negotiate(&six, &[Codec::Pcm], &ALL, &c),
            Err(Refusal::Channels { wanted: 6, max: 2 })
        );
        let float = Source {
            sample_format: SampleFormat::PcmF32Le,
            ..STEREO_48K
        };
        assert_eq!(
            negotiate(&float, &[Codec::Pcm], &ALL, &c),
            Err(Refusal::SampleFormat {
                wanted: SampleFormat::PcmF32Le
            })
        );
        let e = negotiate(&STEREO_48K, &[Codec::Opus], &ALL, &c).unwrap_err();
        assert!(e.to_string().contains("opus"), "{}", e);
    }
}
