//! HLS playlists (RFC 8216, https://www.rfc-editor.org/rfc/rfc8216, read
//! 2026-10-03): the parser, the choice of a variant, and where a live
//! playlist is entered. Pure: text in, a value or a named error out.
//!
//! What is read: `EXTM3U` (section 4.3.1.1), `EXT-X-VERSION` (4.3.1.2),
//! `EXTINF` (4.3.2.1), `EXT-X-DISCONTINUITY` (4.3.2.3, tolerated),
//! `EXT-X-KEY` (4.3.2.4, `METHOD=NONE` only), `EXT-X-TARGETDURATION`
//! (4.3.3.1), `EXT-X-MEDIA-SEQUENCE` (4.3.3.2), `EXT-X-ENDLIST` (4.3.3.4) and
//! `EXT-X-STREAM-INF` (4.3.4.2). What is refused by name: `EXT-X-KEY` with
//! any other method, `EXT-X-MAP` (4.3.2.5), `EXT-X-BYTERANGE` (4.3.2.2),
//! `EXT-X-I-FRAMES-ONLY` (4.3.3.6) and a protocol version above 7. Every
//! other tag is ignored, as section 6.3.1 requires of unrecognised tags.

use crate::error::FetchError;

/// The highest `EXT-X-VERSION` this parser reads: the highest RFC 8216
/// section 7 defines. Section 6.3.1: a client "MUST NOT attempt to use the
/// Playlist" of a version it does not support.
pub const MAX_VERSION: u64 = 7;

/// One `EXT-X-STREAM-INF` and its URI line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// `BANDWIDTH`, bits per second.
    pub bandwidth: u64,
    /// `CODECS`, when given.
    pub codecs: Option<String>,
    /// The media playlist's URI, as written (may be relative).
    pub uri: String,
}

/// One media segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The segment's URI, as written (may be relative).
    pub uri: String,
    /// Its `EXTINF` duration, in milliseconds.
    pub duration_ms: u64,
    /// Whether `EXT-X-DISCONTINUITY` applies to it.
    pub discontinuity: bool,
}

/// A media playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Media {
    /// `EXT-X-TARGETDURATION`, seconds.
    pub target_duration: u64,
    /// The media sequence number of the first segment listed (0 when the tag
    /// is absent, section 4.3.3.2).
    pub media_sequence: u64,
    /// The segments, in order.
    pub segments: Vec<Segment>,
    /// Whether `EXT-X-ENDLIST` is present: no more segments will be added.
    pub ended: bool,
}

/// A playlist is one or the other (section 4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Playlist {
    /// Every URI line is a media playlist.
    Master(Vec<Variant>),
    /// Every URI line is a media segment.
    Media(Media),
}

fn malformed(what: &str) -> FetchError {
    FetchError::Malformed(format!("hls: {what}"))
}

fn unsupported(what: &str) -> FetchError {
    FetchError::Unsupported(format!("hls: {what}"))
}

/// Whether a body is a playlist: it starts with `#EXTM3U` (section 4.3.1.1).
pub fn looks_like_playlist(start: &[u8]) -> bool {
    start.starts_with(b"#EXTM3U")
}

/// Whether a `Content-Type` is one playlists are served with. The first is
/// the RFC's (section 4: "application/vnd.apple.mpegurl" or "audio/mpegurl");
/// the `x-` forms are what servers also send (ASSUMED from practice).
pub fn is_playlist_type(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or("").trim();
    [
        "application/vnd.apple.mpegurl",
        "audio/mpegurl",
        "application/x-mpegurl",
        "audio/x-mpegurl",
    ]
    .iter()
    .any(|t| essence.eq_ignore_ascii_case(t))
}

/// An attribute list (section 4.2): `NAME=value` pairs separated by commas,
/// where a quoted value may itself hold commas. Quotes are removed.
fn attributes(list: &str) -> Result<Vec<(&str, &str)>, FetchError> {
    let mut out: Vec<(&str, &str)> = Vec::new();
    let mut rest = list.trim();
    while !rest.is_empty() {
        let Some((name, after)) = rest.split_once('=') else {
            return Err(malformed("an attribute without a value"));
        };
        let name = name.trim();
        let (value, tail) = if let Some(quoted) = after.strip_prefix('"') {
            let Some((value, tail)) = quoted.split_once('"') else {
                return Err(malformed("an unclosed quoted attribute"));
            };
            (value, tail)
        } else {
            match after.split_once(',') {
                Some((value, tail)) => (value, tail),
                None => (after, ""),
            }
        };
        let tail = tail.trim_start();
        rest = tail.strip_prefix(',').unwrap_or(tail);
        // "A given AttributeName MUST NOT appear more than once ... Clients
        // SHOULD refuse to parse such Playlists" (section 4.2).
        if out.iter().any(|(seen, _)| *seen == name) {
            return Err(malformed("an attribute appears twice in one tag"));
        }
        out.push((name, value.trim()));
    }
    Ok(out)
}

fn attribute<'a>(attrs: &[(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

fn integer(text: &str, what: &str) -> Result<u64, FetchError> {
    let text = text.trim();
    match text.parse::<u64>() {
        Ok(n) if text.bytes().all(|b| b.is_ascii_digit()) => Ok(n),
        _ => Err(malformed(&format!("{what} is not a decimal integer"))),
    }
}

/// An `EXTINF` duration (decimal-integer or decimal-floating-point seconds,
/// section 4.3.2.1) in milliseconds, without going through a float.
fn duration_ms(text: &str) -> Option<u64> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole
        .bytes()
        .chain(fraction.bytes())
        .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let mut millis = 0u64;
    for i in 0..3 {
        millis = millis * 10 + u64::from(fraction.as_bytes().get(i).map_or(0, |b| b - b'0'));
    }
    whole.checked_mul(1000)?.checked_add(millis)
}

/// Parses a playlist. The text must be UTF-8 without a byte order mark and
/// start with `#EXTM3U` (sections 4.1 and 4.3.1.1).
pub fn parse(text: &str) -> Result<Playlist, FetchError> {
    if text.starts_with('\u{feff}') {
        return Err(malformed("the playlist starts with a byte order mark"));
    }
    let mut lines = text.lines().map(str::trim_end);
    if lines.next() != Some("#EXTM3U") {
        return Err(malformed("the playlist does not start with #EXTM3U"));
    }

    let mut version_seen = false;
    let mut variants: Vec<Variant> = Vec::new();
    let mut pending_variant: Option<(u64, Option<String>)> = None;
    let mut master_tags = false;
    let mut media_tags = false;
    let mut hls_tags = false;
    let mut target_duration: Option<u64> = None;
    let mut media_sequence: Option<u64> = None;
    let mut ended = false;
    let mut segments: Vec<(String, Option<u64>, bool)> = Vec::new();
    let mut pending_duration: Option<Option<u64>> = None;
    let mut pending_discontinuity = false;

    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some(tag) = line.strip_prefix("#EXT") {
            let (name, value) = tag.split_once(':').unwrap_or((tag, ""));
            hls_tags |= name.starts_with("-X-");
            match name {
                "-X-VERSION" => {
                    // "If a client encounters a Playlist with multiple
                    // EXT-X-VERSION tags, it MUST fail to parse it" (4.3.1.2).
                    if version_seen {
                        return Err(malformed("more than one EXT-X-VERSION"));
                    }
                    version_seen = true;
                    let version = integer(value, "EXT-X-VERSION")?;
                    if version > MAX_VERSION {
                        return Err(unsupported(&format!(
                            "protocol version {version} (EXT-X-VERSION above {MAX_VERSION})"
                        )));
                    }
                }
                "-X-STREAM-INF" => {
                    master_tags = true;
                    let attrs = attributes(value)?;
                    // "Every EXT-X-STREAM-INF tag MUST include the BANDWIDTH
                    // attribute" (4.3.4.2).
                    let Some(bandwidth) = attribute(&attrs, "BANDWIDTH") else {
                        return Err(malformed("EXT-X-STREAM-INF without BANDWIDTH"));
                    };
                    pending_variant = Some((
                        integer(bandwidth, "BANDWIDTH")?,
                        attribute(&attrs, "CODECS").map(str::to_string),
                    ));
                }
                "-X-MEDIA" | "-X-I-FRAME-STREAM-INF" | "-X-SESSION-DATA" | "-X-SESSION-KEY" => {
                    master_tags = true;
                }
                "INF" => {
                    media_tags = true;
                    let duration = value.split(',').next().unwrap_or("");
                    pending_duration = Some(duration_ms(duration));
                }
                "-X-TARGETDURATION" => {
                    media_tags = true;
                    target_duration = Some(integer(value, "EXT-X-TARGETDURATION")?);
                }
                "-X-MEDIA-SEQUENCE" => {
                    media_tags = true;
                    media_sequence = Some(integer(value, "EXT-X-MEDIA-SEQUENCE")?);
                }
                "-X-ENDLIST" => {
                    media_tags = true;
                    ended = true;
                }
                "-X-DISCONTINUITY" => {
                    media_tags = true;
                    pending_discontinuity = true;
                }
                "-X-KEY" => {
                    media_tags = true;
                    let attrs = attributes(value)?;
                    match attribute(&attrs, "METHOD") {
                        Some("NONE") => {}
                        Some(method) => {
                            return Err(unsupported(&format!(
                                "encrypted segments (EXT-X-KEY METHOD={method})"
                            )))
                        }
                        // "This attribute is REQUIRED" (4.3.2.4).
                        None => return Err(malformed("EXT-X-KEY without METHOD")),
                    }
                }
                "-X-MAP" => return Err(unsupported("fragmented mp4 segments (EXT-X-MAP)")),
                "-X-BYTERANGE" => return Err(unsupported("byte-range segments (EXT-X-BYTERANGE)")),
                "-X-I-FRAMES-ONLY" => {
                    return Err(unsupported("an I-frame playlist (EXT-X-I-FRAMES-ONLY)"))
                }
                "-X-DISCONTINUITY-SEQUENCE"
                | "-X-PLAYLIST-TYPE"
                | "-X-PROGRAM-DATE-TIME"
                | "-X-DATERANGE" => media_tags = true,
                // "ignore any unrecognized tags" (6.3.1).
                _ => {}
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some((bandwidth, codecs)) = pending_variant.take() {
            variants.push(Variant {
                bandwidth,
                codecs,
                uri: line.to_string(),
            });
        } else {
            segments.push((
                line.to_string(),
                pending_duration.take().flatten(),
                std::mem::take(&mut pending_discontinuity),
            ));
        }
    }

    // "Clients MUST fail to parse Playlists that contain both Media Segment
    // tags and Master Playlist tags" (4.3.2).
    if master_tags && (media_tags || !segments.is_empty()) {
        return Err(malformed("both master playlist tags and media segments"));
    }
    if master_tags {
        if variants.is_empty() {
            return Err(malformed("a master playlist with no variant"));
        }
        return Ok(Playlist::Master(variants));
    }
    if !hls_tags {
        // A list of URLs with at most EXTINF lines is the older M3U, which
        // names streams, not segments.
        return Err(FetchError::Unsupported(
            "m3u: a plain playlist, not HLS (no EXT-X-TARGETDURATION); use the stream's own url"
                .into(),
        ));
    }
    // "The EXT-X-TARGETDURATION tag is REQUIRED" (4.3.3.1).
    let Some(target_duration) = target_duration else {
        return Err(malformed("a media playlist without EXT-X-TARGETDURATION"));
    };
    let segments = segments
        .into_iter()
        .map(|(uri, duration, discontinuity)| match duration {
            Some(duration_ms) => Ok(Segment {
                uri,
                duration_ms,
                discontinuity,
            }),
            // "This tag is REQUIRED for each Media Segment" (4.3.2.1).
            None => Err(malformed("a segment without a valid EXTINF")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Playlist::Media(Media {
        target_duration,
        media_sequence: media_sequence.unwrap_or(0),
        segments,
        ended,
    }))
}

/// How chorus reads one entry of a `CODECS` list (RFC 6381 names, as section
/// 4.3.4.2 says).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodecKind {
    /// MPEG audio layer 3, which chorus decodes.
    Mp3,
    /// MPEG-4 AAC in any profile, which chorus never decodes.
    Aac,
    /// Anything else: video, AC-3, or a name not known here.
    Other,
}

/// `mp4a.40.34` is MPEG-4 audio object type 34, "MPEG-1/2 Layer 3", and is
/// what Apple's authoring guidance writes for MP3
/// (https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices-appendixes,
/// read 2026-10-03). ASSUMED from the MP4 registration authority's object
/// type table, not re-read: `mp4a.69` (MPEG-2 audio part 3) and `mp4a.6B`
/// (MPEG-1 audio part 3) also name MP3. Every other `mp4a.40.x` is an AAC
/// family object type.
fn codec_kind(codec: &str) -> CodecKind {
    let codec = codec.trim().to_ascii_lowercase();
    match codec.as_str() {
        "mp4a.40.34" | "mp4a.69" | "mp4a.6b" | "mp3" => CodecKind::Mp3,
        c if c.starts_with("mp4a.40.") => CodecKind::Aac,
        _ => CodecKind::Other,
    }
}

/// The variant to play: the highest `BANDWIDTH` among those whose `CODECS`,
/// when given, are all MP3 (a variant without `CODECS` is taken and its
/// segments are judged when they arrive). When nothing is playable the error
/// names why: AAC when every variant carries AAC, the codecs otherwise.
pub fn choose_variant(variants: &[Variant]) -> Result<&Variant, FetchError> {
    let kinds = |v: &Variant| -> Vec<CodecKind> {
        v.codecs
            .as_deref()
            .map(|list| list.split(',').map(codec_kind).collect())
            .unwrap_or_default()
    };
    let playable = variants
        .iter()
        .filter(|v| kinds(v).iter().all(|k| *k == CodecKind::Mp3))
        .max_by_key(|v| v.bandwidth);
    if let Some(variant) = playable {
        return Ok(variant);
    }
    let best = variants.iter().max_by_key(|v| v.bandwidth);
    let codecs = best.and_then(|v| v.codecs.as_deref()).unwrap_or("");
    if variants.iter().all(|v| kinds(v).contains(&CodecKind::Aac)) {
        let aac = codecs
            .split(',')
            .map(str::trim)
            .find(|c| codec_kind(c) == CodecKind::Aac)
            .unwrap_or("mp4a.40");
        return Err(unsupported(&format!("aac ({aac})")));
    }
    Err(unsupported(&format!(
        "no playable variant (codecs {codecs})"
    )))
}

/// Where to enter a media playlist. An ended playlist is played from its
/// start. A live one is entered so that at least three target durations
/// remain after the chosen segment's start: "the client SHOULD NOT choose a
/// segment that starts less than three target durations from the end of the
/// Playlist file" (section 6.3.3). The latest such segment is chosen; a
/// playlist shorter than that is played from its start.
pub fn start_index(media: &Media) -> usize {
    if media.ended {
        return 0;
    }
    let need = media.target_duration.saturating_mul(3000);
    let mut remaining = 0u64;
    for (i, segment) in media.segments.iter().enumerate().rev() {
        remaining = remaining.saturating_add(segment.duration_ms);
        if remaining >= need {
            return i;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/playlists/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn media(name: &str) -> Media {
        match parse(&fixture(name)) {
            Ok(Playlist::Media(media)) => media,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn master(name: &str) -> Vec<Variant> {
        match parse(&fixture(name)) {
            Ok(Playlist::Master(variants)) => variants,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn unsupported_words(name: &str) -> String {
        match parse(&fixture(name)) {
            Err(FetchError::Unsupported(what)) => what,
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn a_vod_playlist_parses() {
        let m = media("vod.m3u8");
        assert_eq!(m.target_duration, 10);
        assert_eq!(m.media_sequence, 0);
        assert!(m.ended);
        assert_eq!(
            m.segments,
            vec![
                Segment {
                    uri: "seg0.mp3".into(),
                    duration_ms: 9009,
                    discontinuity: false
                },
                Segment {
                    uri: "seg1.mp3".into(),
                    duration_ms: 9009,
                    discontinuity: false
                },
                Segment {
                    uri: "http://media.example/hls/seg2.mp3?t=1".into(),
                    duration_ms: 3003,
                    discontinuity: true
                },
            ]
        );
        assert_eq!(start_index(&m), 0);
    }

    #[test]
    fn a_live_playlist_parses_and_is_entered_three_target_durations_back() {
        let m = media("live.m3u8");
        assert_eq!(m.target_duration, 6);
        assert_eq!(m.media_sequence, 2680);
        assert!(!m.ended);
        assert_eq!(m.segments.len(), 6);
        assert_eq!(m.segments[0].duration_ms, 6000);
        // Six 6 s segments: the fourth leaves exactly 18 s after its start.
        assert_eq!(start_index(&m), 3);
        let short = Media {
            segments: m.segments[..2].to_vec(),
            ..m.clone()
        };
        assert_eq!(start_index(&short), 0);
        let empty = Media {
            segments: Vec::new(),
            ..m
        };
        assert_eq!(start_index(&empty), 0);
    }

    #[test]
    fn crlf_lines_and_integer_durations_parse() {
        let m = match parse(
            "#EXTM3U\r\n#EXT-X-TARGETDURATION:8\r\n#EXTINF:8,\r\na.mp3\r\n#EXTINF:7\r\nb.mp3\r\n",
        ) {
            Ok(Playlist::Media(m)) => m,
            other => panic!("{other:?}"),
        };
        assert_eq!(m.segments[0].duration_ms, 8000);
        assert_eq!(m.segments[1].uri, "b.mp3");
        assert!(!m.ended);
    }

    #[test]
    fn durations_convert_without_floats() {
        assert_eq!(duration_ms("9.009"), Some(9009));
        assert_eq!(duration_ms("10"), Some(10000));
        assert_eq!(duration_ms("0.5"), Some(500));
        assert_eq!(duration_ms("2.99999"), Some(2999));
        assert_eq!(duration_ms(".25"), Some(250));
        assert_eq!(duration_ms("-1"), None);
        assert_eq!(duration_ms(""), None);
        assert_eq!(duration_ms("1e3"), None);
    }

    #[test]
    fn a_master_playlist_parses_and_the_best_mp3_variant_is_chosen() {
        let variants = master("master-mp3.m3u8");
        assert_eq!(variants.len(), 4);
        assert_eq!(variants[0].codecs.as_deref(), Some("mp4a.40.34"));
        let chosen = choose_variant(&variants).unwrap();
        // 320k is AAC and 256k has video; the best MP3-only one is 192k.
        assert_eq!(
            (chosen.bandwidth, chosen.uri.as_str()),
            (192000, "mp3-192/index.m3u8")
        );
    }

    #[test]
    fn a_variant_without_codecs_is_playable_until_its_segments_say_otherwise() {
        let variants = master("master-no-codecs.m3u8");
        assert_eq!(choose_variant(&variants).unwrap().uri, "hi/index.m3u8");
    }

    #[test]
    fn an_all_aac_master_is_refused_by_name() {
        let variants = master("master-aac.m3u8");
        match choose_variant(&variants) {
            Err(FetchError::Unsupported(what)) => assert_eq!(what, "hls: aac (mp4a.40.2)"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_master_with_nothing_playable_and_not_all_aac_names_the_codecs() {
        let variants = vec![
            Variant {
                bandwidth: 1,
                codecs: Some("mp4a.40.2".into()),
                uri: "a".into(),
            },
            Variant {
                bandwidth: 2,
                codecs: Some("ac-3".into()),
                uri: "b".into(),
            },
        ];
        match choose_variant(&variants) {
            Err(FetchError::Unsupported(what)) => {
                assert_eq!(what, "hls: no playable variant (codecs ac-3)")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn codec_names_are_read_case_insensitively() {
        assert_eq!(codec_kind("mp4a.40.34"), CodecKind::Mp3);
        assert_eq!(codec_kind(" mp4a.6B"), CodecKind::Mp3);
        assert_eq!(codec_kind("mp4a.69"), CodecKind::Mp3);
        assert_eq!(codec_kind("mp4a.40.2"), CodecKind::Aac);
        assert_eq!(codec_kind("mp4a.40.5"), CodecKind::Aac);
        assert_eq!(codec_kind("MP4A.40.29"), CodecKind::Aac);
        assert_eq!(codec_kind("avc1.64001f"), CodecKind::Other);
        assert_eq!(codec_kind("ec-3"), CodecKind::Other);
    }

    #[test]
    fn each_refused_tag_has_its_words() {
        assert_eq!(
            unsupported_words("key-aes128.m3u8"),
            "hls: encrypted segments (EXT-X-KEY METHOD=AES-128)"
        );
        assert_eq!(
            unsupported_words("key-sample-aes.m3u8"),
            "hls: encrypted segments (EXT-X-KEY METHOD=SAMPLE-AES)"
        );
        assert_eq!(
            unsupported_words("fmp4.m3u8"),
            "hls: fragmented mp4 segments (EXT-X-MAP)"
        );
        assert_eq!(
            unsupported_words("byterange.m3u8"),
            "hls: byte-range segments (EXT-X-BYTERANGE)"
        );
        assert_eq!(
            unsupported_words("version-8.m3u8"),
            "hls: protocol version 8 (EXT-X-VERSION above 7)"
        );
        assert_eq!(
            unsupported_words("plain.m3u"),
            "m3u: a plain playlist, not HLS (no EXT-X-TARGETDURATION); use the stream's own url"
        );
    }

    #[test]
    fn key_method_none_is_not_encryption() {
        let m = media("key-none.m3u8");
        assert_eq!(m.segments.len(), 2);
    }

    #[test]
    fn what_the_rfc_says_must_fail_fails() {
        for (text, why) in [
            (
                "#EXT-X-TARGETDURATION:5\n#EXTINF:5,\na.mp3\n",
                "does not start with #EXTM3U",
            ),
            (
                "\u{feff}#EXTM3U\n#EXT-X-TARGETDURATION:5\n",
                "byte order mark",
            ),
            (
                "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:5\n",
                "more than one EXT-X-VERSION",
            ),
            (
                "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nv.m3u8\n#EXTINF:5,\na.mp3\n",
                "both master",
            ),
            (
                "#EXTM3U\n#EXT-X-STREAM-INF:CODECS=\"mp4a.40.34\"\nv.m3u8\n",
                "without BANDWIDTH",
            ),
            (
                "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1,BANDWIDTH=2\nv.m3u8\n",
                "appears twice",
            ),
            (
                "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1,CODECS=\"mp4a\nv.m3u8\n",
                "unclosed quoted",
            ),
            ("#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO\n", "no variant"),
            (
                "#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:1\n#EXTINF:5,\na.mp3\n",
                "without EXT-X-TARGETDURATION",
            ),
            (
                "#EXTM3U\n#EXT-X-TARGETDURATION:5\na.mp3\n",
                "without a valid EXTINF",
            ),
            (
                "#EXTM3U\n#EXT-X-TARGETDURATION:5\n#EXTINF:-1,\na.mp3\n",
                "without a valid EXTINF",
            ),
            (
                "#EXTM3U\n#EXT-X-TARGETDURATION:five\n",
                "not a decimal integer",
            ),
            (
                "#EXTM3U\n#EXT-X-TARGETDURATION:5\n#EXT-X-KEY:URI=\"k\"\n",
                "EXT-X-KEY without METHOD",
            ),
        ] {
            match parse(text) {
                Err(FetchError::Malformed(got)) => assert!(got.contains(why), "{got} / {why}"),
                other => panic!("{why}: {other:?}"),
            }
        }
    }

    #[test]
    fn unknown_tags_and_comments_are_ignored() {
        let text = "#EXTM3U\n# a comment\n#EXT-X-FUTURE-TAG:1\n#EXT-X-TARGETDURATION:5\n\n\
                    #EXT-X-INDEPENDENT-SEGMENTS\n#EXT-X-START:TIME-OFFSET=0\n#EXTINF:5,title, with comma\na.mp3\n";
        match parse(text) {
            Ok(Playlist::Media(m)) => assert_eq!(m.segments.len(), 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn playlists_are_recognised_by_type_and_by_their_first_line() {
        for t in [
            "application/vnd.apple.mpegurl",
            "Application/VND.Apple.MpegURL; charset=utf-8",
            "audio/mpegurl",
            "application/x-mpegurl",
            "audio/x-mpegURL",
        ] {
            assert!(is_playlist_type(t), "{t}");
        }
        for t in [
            "audio/mpeg",
            "application/octet-stream",
            "",
            "audio/mpegurlx",
        ] {
            assert!(!is_playlist_type(t), "{t}");
        }
        assert!(looks_like_playlist(b"#EXTM3U\n#EXT"));
        assert!(!looks_like_playlist(b"ID3\x04"));
        assert!(!looks_like_playlist(b"#EXT"));
    }

    #[test]
    fn attribute_lists_keep_commas_inside_quotes() {
        let attrs =
            attributes("BANDWIDTH=128000,CODECS=\"avc1.4d401e,mp4a.40.2\",RESOLUTION=640x360")
                .unwrap();
        assert_eq!(attribute(&attrs, "BANDWIDTH"), Some("128000"));
        assert_eq!(attribute(&attrs, "CODECS"), Some("avc1.4d401e,mp4a.40.2"));
        assert_eq!(attribute(&attrs, "RESOLUTION"), Some("640x360"));
        assert_eq!(attribute(&attrs, "AUDIO"), None);
    }
}
