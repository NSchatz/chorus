//! HLS (RFC 8216, https://www.rfc-editor.org/rfc/rfc8216, read 2026-10-03) in
//! its smallest correct scope: a media playlist of packed-audio MP3 segments
//! (section 3.4), read as one continuous MPEG audio stream.
//!
//! The stream fetches a segment, strips the ID3 tag that carries its
//! timestamp, hands out its bytes, and moves to the next; when a live
//! playlist runs out it is reloaded on the schedule of section 6.3.4. What a
//! segment turns out to be is judged from its first bytes, and everything
//! that is not MPEG audio is refused by name. Every playlist and segment URL
//! goes through the same request path as any other, so the fetch policy
//! applies to each.

pub mod playlist;

use std::collections::VecDeque;
use std::io::{self, Read};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::FetchError;
use crate::http::{self, Body, Ctx, Transport};
use crate::url::Url;
use playlist::{Media, Playlist};

/// The most bytes a playlist may be. ASSUMED: a day-long event playlist of
/// two-second segments is a few hundred kilobytes.
pub const MAX_PLAYLIST_BYTES: usize = 1024 * 1024;

/// The most ID3 bytes skipped at the start of one segment. ASSUMED: the
/// timestamp tag is 73 bytes; a station may add text frames or a small image.
pub const MAX_ID3_BYTES: u64 = 1024 * 1024;

/// The shortest wait between two loads of a playlist, whatever its target
/// duration says. ASSUMED: it only keeps a playlist that declares a target
/// duration of zero from being reloaded in a tight loop.
const MIN_RELOAD_WAIT: Duration = Duration::from_millis(250);

/// How long a live playlist may stay unchanged, in target durations, before
/// the stream is given up. ASSUMED: the RFC sets no bound; a server publishes
/// a new version every 0.5 to 1.5 target durations (section 6.2.1), so six
/// without one is a stalled encoder, not jitter.
const STALL_TARGET_DURATIONS: u32 = 6;

/// What the first bytes of a segment (after any ID3 tag) say it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    /// MPEG audio frames (layer 1, 2 or 3): what chorus plays.
    MpegAudio,
    /// AAC in ADTS framing.
    Adts,
    /// AC-3 or Enhanced AC-3.
    Ac3,
    /// An MPEG-2 transport stream.
    TransportStream,
    /// Fragmented MPEG-4.
    FragmentedMp4,
    /// Nothing recognised.
    Unknown,
}

/// Classifies a segment by its first bytes.
///
/// MPEG audio and ADTS both start with a sync word of set bits; the two
/// "layer" bits after the version bit are `00` in ADTS and never `00` in
/// MPEG audio (ASSUMED from the header layouts of ISO/IEC 11172-3 and
/// 13818-7, which are not free to read; the same test is described in the
/// research note this goal started from). A transport stream packet starts
/// with the sync byte 0x47; an MPEG-4 file with a box whose type is at bytes
/// 4 to 8; AC-3 with the sync word 0x0B77 (ASSUMED likewise).
pub fn classify_segment(start: &[u8]) -> SegmentKind {
    if start.len() >= 8 && matches!(&start[4..8], b"ftyp" | b"styp" | b"moof" | b"sidx") {
        return SegmentKind::FragmentedMp4;
    }
    match start {
        [0x47, ..] => SegmentKind::TransportStream,
        [0x0b, 0x77, ..] => SegmentKind::Ac3,
        [0xff, b, ..] if b & 0xf6 == 0xf0 => SegmentKind::Adts,
        [0xff, b, ..] if b & 0xe0 == 0xe0 && b & 0x06 != 0 => SegmentKind::MpegAudio,
        _ => SegmentKind::Unknown,
    }
}

/// The whole length of the ID3v2 tag that `start` begins with, header
/// included, or `None` when it does not begin with one. The ID3v2 header
/// (https://id3.org/id3v2.4.0-structure section 3.1, read 2026-10-03):
/// `"ID3"`, two version bytes, one flags byte, and the size of what follows
/// the header as four bytes of seven bits each ("zz is less than $80"); flag
/// bit 4 says a ten-byte footer follows the tag, which the size leaves out.
pub fn id3_tag_len(start: &[u8]) -> Option<u64> {
    if start.len() < 10 || &start[..3] != b"ID3" {
        return None;
    }
    if start[6..10].iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    let size = start[6..10]
        .iter()
        .fold(0u64, |acc, b| (acc << 7) | u64::from(*b));
    let footer = if start[5] & 0x10 != 0 { 10 } else { 0 };
    Some(10 + size + footer)
}

fn unsupported(what: &str) -> FetchError {
    FetchError::Unsupported(format!("hls: {what}"))
}

/// Reads a whole playlist body, bounded, as UTF-8 (RFC 8216 section 4.1).
pub(crate) fn read_playlist(body: &mut Body<Transport>) -> Result<String, FetchError> {
    let mut bytes = Vec::new();
    body.by_ref()
        .take(MAX_PLAYLIST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PLAYLIST_BYTES {
        return Err(FetchError::Malformed(format!(
            "hls: a playlist larger than {MAX_PLAYLIST_BYTES} bytes"
        )));
    }
    String::from_utf8(bytes)
        .map_err(|_| FetchError::Malformed("hls: a playlist that is not UTF-8".into()))
}

/// The concatenated segments of one media playlist.
pub(crate) struct HlsStream {
    ctx: Ctx,
    /// The media playlist's URL: what is reloaded and what its entries are
    /// relative to.
    playlist_url: Url,
    target: Duration,
    /// Segments not yet fetched.
    queue: VecDeque<Url>,
    /// The media sequence number of the first segment not yet queued.
    next_sequence: u64,
    ended: bool,
    /// When the last load of the playlist began (section 6.3.4 measures from
    /// there), on the monotonic clock.
    last_load: Instant,
    /// Whether that load found the playlist changed.
    last_changed: bool,
    /// When the playlist last changed.
    changed_at: Instant,
    current: Option<Body<Transport>>,
}

impl HlsStream {
    /// Starts a stream from a playlist already fetched from `url` (the URL
    /// that answered, after redirects) in a load that began at `began`.
    pub fn start(
        mut ctx: Ctx,
        url: Url,
        text: &str,
        began: Instant,
    ) -> Result<HlsStream, FetchError> {
        let (playlist_url, media, began) = match playlist::parse(text)? {
            Playlist::Media(media) => (url, media, began),
            Playlist::Master(variants) => {
                let variant = playlist::choose_variant(&variants)?;
                let variant_url = url.join(&variant.uri)?;
                let began = Instant::now();
                let (final_url, text) = load_playlist(&mut ctx, &variant_url)?;
                match playlist::parse(&text)? {
                    Playlist::Media(media) => (final_url, media, began),
                    Playlist::Master(_) => {
                        return Err(FetchError::Malformed(
                            "hls: a master playlist's variant is itself a master playlist".into(),
                        ))
                    }
                }
            }
        };
        let first = playlist::start_index(&media);
        let mut queue = VecDeque::new();
        for segment in &media.segments[first..] {
            queue.push_back(playlist_url.join(&segment.uri)?);
        }
        let mut stream = HlsStream {
            ctx,
            playlist_url,
            target: Duration::from_secs(media.target_duration),
            queue,
            next_sequence: media
                .media_sequence
                .saturating_add(media.segments.len() as u64),
            ended: media.ended,
            last_load: began,
            last_changed: true,
            changed_at: began,
            current: None,
        };
        // The first segment is opened here, so a stream that cannot be played
        // is refused by `open` and not by the first read.
        stream.advance()?;
        Ok(stream)
    }

    /// The media playlist's URL.
    pub fn playlist_url(&self) -> &Url {
        &self.playlist_url
    }

    /// Makes the next segment current. `Ok(false)`: the playlist has ended
    /// and every segment was read.
    fn advance(&mut self) -> Result<bool, FetchError> {
        loop {
            if let Some(url) = self.queue.pop_front() {
                self.current = Some(open_segment(&mut self.ctx, &url)?);
                return Ok(true);
            }
            if self.ended {
                self.current = None;
                return Ok(false);
            }
            self.reload()?;
        }
    }

    /// Reloads the playlist, no sooner than RFC 8216 section 6.3.4 allows:
    /// after a load that found the playlist changed (the first load counts)
    /// "the client MUST wait for at least the target duration before
    /// attempting to reload the Playlist file again, measured from the last
    /// time the client began loading the Playlist file"; after one that found
    /// it unchanged "it MUST wait for a period of one-half the target
    /// duration before retrying". The next segment is the one with the lowest
    /// media sequence number above the last one loaded (section 6.3.5).
    fn reload(&mut self) -> Result<(), FetchError> {
        let wait = reload_wait(self.target, self.last_changed);
        let due = self.last_load + wait;
        let now = Instant::now();
        if due > now {
            thread::sleep(due - now);
        }
        let began = Instant::now();
        let (_, text) = load_playlist(&mut self.ctx, &self.playlist_url)?;
        let media = match playlist::parse(&text)? {
            Playlist::Media(media) => media,
            Playlist::Master(_) => {
                return Err(FetchError::Malformed(
                    "hls: a media playlist became a master playlist".into(),
                ))
            }
        };
        self.last_load = began;
        let fresh = new_segments(&media, self.next_sequence);
        let mut changed = false;
        for segment in &media.segments[fresh..] {
            self.queue.push_back(self.playlist_url.join(&segment.uri)?);
            changed = true;
        }
        if changed {
            self.next_sequence = media
                .media_sequence
                .saturating_add(media.segments.len() as u64);
        }
        if media.ended && !self.ended {
            self.ended = true;
            changed = true;
        }
        self.target = Duration::from_secs(media.target_duration);
        self.last_changed = changed;
        if changed {
            self.changed_at = began;
        } else if began.duration_since(self.changed_at) > self.target * STALL_TARGET_DURATIONS {
            return Err(FetchError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "hls: the live playlist has not changed for {STALL_TARGET_DURATIONS} target durations"
                ),
            )));
        }
        Ok(())
    }
}

/// How long to wait after a load of the playlist began before loading it
/// again (section 6.3.4): the target duration after a load that found it
/// changed, half of it after one that did not.
pub fn reload_wait(target: Duration, last_load_changed: bool) -> Duration {
    let wait = if last_load_changed {
        target
    } else {
        target / 2
    };
    wait.max(MIN_RELOAD_WAIT)
}

/// The index of the first segment in a reloaded playlist that has not been
/// queued yet, given the sequence number of the first one not yet queued. A
/// reader that fell behind the window (its next segment is gone) resumes at
/// the window's start; a playlist entirely behind the reader has nothing new.
pub fn new_segments(media: &Media, next_sequence: u64) -> usize {
    let skip = next_sequence.saturating_sub(media.media_sequence);
    usize::try_from(skip)
        .unwrap_or(usize::MAX)
        .min(media.segments.len())
}

fn load_playlist(ctx: &mut Ctx, url: &Url) -> Result<(Url, String), FetchError> {
    let mut response = http::get(ctx, url, None, false)?;
    let text = read_playlist(&mut response.body)?;
    Ok((response.url, text))
}

/// Fetches a segment and positions its body at the first audio byte: past
/// the ID3 tag (or tags) that section 3.4 puts at the start of every packed
/// audio segment. Anything that is not MPEG audio is refused by name.
fn open_segment(ctx: &mut Ctx, url: &Url) -> Result<Body<Transport>, FetchError> {
    let mut body = http::get(ctx, url, None, false)?.body;
    let mut skipped = 0u64;
    loop {
        let start = body.peek(10)?.to_vec();
        if start.is_empty() {
            return Ok(body);
        }
        if let Some(len) = id3_tag_len(&start) {
            skipped += len;
            if skipped > MAX_ID3_BYTES {
                return Err(FetchError::Malformed(format!(
                    "hls: more than {MAX_ID3_BYTES} bytes of ID3 at the start of a segment"
                )));
            }
            body.skip(len)?;
            continue;
        }
        return match classify_segment(&start) {
            SegmentKind::MpegAudio => Ok(body),
            SegmentKind::Adts => Err(unsupported("aac (adts segments)")),
            SegmentKind::Ac3 => Err(unsupported("ac-3 segments")),
            SegmentKind::TransportStream => Err(unsupported("mpeg-2 transport stream segments")),
            SegmentKind::FragmentedMp4 => Err(unsupported("fragmented mp4 segments")),
            SegmentKind::Unknown => Err(unsupported("segments that are not packed mp3 audio")),
        };
    }
}

impl Read for HlsStream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            match &mut self.current {
                Some(body) => {
                    let n = body.read(out)?;
                    if n > 0 {
                        return Ok(n);
                    }
                }
                None if self.ended && self.queue.is_empty() => return Ok(0),
                None => {}
            }
            if !self.advance().map_err(FetchError::into_io)? {
                return Ok(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playlist::Segment;

    #[test]
    fn segments_are_classified_by_their_first_bytes() {
        // MPEG-1 layer 3, with and without CRC; MPEG-2 layer 3; MPEG-1 layer 2.
        for start in [
            [0xff, 0xfb, 0x90, 0x00],
            [0xff, 0xfa, 0x90, 0x00],
            [0xff, 0xf3, 0x90, 0x00],
            [0xff, 0xfd, 0x90, 0x00],
        ] {
            assert_eq!(
                classify_segment(&start),
                SegmentKind::MpegAudio,
                "{start:02x?}"
            );
        }
        // MPEG-2.5 layer 3: an eleven-bit sync.
        assert_eq!(
            classify_segment(&[0xff, 0xe3, 0x90, 0x00]),
            SegmentKind::MpegAudio
        );
        // ADTS: MPEG-4 and MPEG-2, with and without CRC.
        for start in [
            [0xff, 0xf1, 0x50, 0x80],
            [0xff, 0xf0, 0x50, 0x80],
            [0xff, 0xf9, 0x50, 0x80],
            [0xff, 0xf8, 0x50, 0x80],
        ] {
            assert_eq!(classify_segment(&start), SegmentKind::Adts, "{start:02x?}");
        }
        assert_eq!(
            classify_segment(&[0x47, 0x40, 0x00, 0x10]),
            SegmentKind::TransportStream
        );
        assert_eq!(
            classify_segment(&[0x0b, 0x77, 0x00, 0x00]),
            SegmentKind::Ac3
        );
        assert_eq!(
            classify_segment(b"\0\0\0\x18ftypmp42"),
            SegmentKind::FragmentedMp4
        );
        assert_eq!(
            classify_segment(b"\0\0\0\x18stypmsdh"),
            SegmentKind::FragmentedMp4
        );
        assert_eq!(
            classify_segment(b"\0\0\0\x18moof\0\0"),
            SegmentKind::FragmentedMp4
        );
        assert_eq!(classify_segment(b"<html>"), SegmentKind::Unknown);
        assert_eq!(classify_segment(&[0xff]), SegmentKind::Unknown);
        assert_eq!(classify_segment(&[]), SegmentKind::Unknown);
    }

    #[test]
    fn id3_tag_lengths_are_syncsafe() {
        assert_eq!(id3_tag_len(b"ID3\x04\0\0\0\0\0\x3f"), Some(73));
        assert_eq!(id3_tag_len(b"ID3\x04\0\0\0\0\x01\x00"), Some(10 + 128));
        assert_eq!(id3_tag_len(b"ID3\x04\0\x10\0\0\0\x05"), Some(25));
        assert_eq!(id3_tag_len(b"ID3\x04\0\0\0\0\0\x80"), None);
        assert_eq!(id3_tag_len(b"ID3\x04\0\0\0\0\0"), None);
        assert_eq!(id3_tag_len(b"\xff\xfb\x90\0\0\0\0\0\0\0"), None);
    }

    #[test]
    fn the_reload_wait_is_the_rfcs() {
        let target = Duration::from_secs(6);
        assert_eq!(reload_wait(target, true), Duration::from_secs(6));
        assert_eq!(reload_wait(target, false), Duration::from_secs(3));
        assert_eq!(reload_wait(Duration::ZERO, true), MIN_RELOAD_WAIT);
        assert_eq!(
            reload_wait(Duration::from_secs(1), false),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn a_reload_finds_what_is_new_by_media_sequence() {
        let segment = |n: u64| Segment {
            uri: format!("s{n}.mp3"),
            duration_ms: 6000,
            discontinuity: false,
        };
        let media = |first: u64, count: u64| Media {
            target_duration: 6,
            media_sequence: first,
            segments: (first..first + count).map(segment).collect(),
            ended: false,
        };
        // Window 10..13, the reader has queued up to 12: nothing new.
        assert_eq!(new_segments(&media(10, 3), 13), 3);
        // One new segment.
        assert_eq!(new_segments(&media(11, 3), 13), 2);
        // The window slid wholly past the reader: resume at its start.
        assert_eq!(new_segments(&media(20, 3), 13), 0);
        // A playlist behind the reader has nothing new.
        assert_eq!(new_segments(&media(5, 3), 13), 3);
    }
}
