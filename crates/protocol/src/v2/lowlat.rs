//! The low-latency path's datagram layer (goal 13, the TV path).
//!
//! `docs/protocol.md` ("Low-latency path") is the contract, and the shared
//! vectors under `fixtures/protocol/lowlat/` hold this implementation and the
//! endpoint's (`firmware/src/lowlat.c`) to the same bytes. What it is, why it
//! is shaped this way and where each number comes from: the decision record
//! named in that section.
//!
//! A pure library: no socket, no clock, no thread. The integration that puts
//! it on UDP (the server's relay, the hub's capture, the Linux client's
//! receive) is a later track, and the API is shaped for it:
//!
//! - a sender keeps a [`FecEncoder`] and a [`Sealer`] per stream: each chunk
//!   goes through [`FecEncoder::push`], every [`Datagram`] that returns goes
//!   through [`Sealer::seal`], and the bytes go out as one UDP payload each;
//! - a receiver keeps an [`Opener`] and a [`FecDecoder`]: each UDP payload
//!   goes through [`Opener::open`] (authentication, header, replay), what it
//!   yields through [`FecDecoder::push`], and every [`Delivered`] chunk goes
//!   to the jitter buffer, which plays it at its stamp and never past its
//!   playout point (the late chunks are the buffer's to count and drop);
//!   [`FecDecoder::expire_before`] closes the groups the playout point has
//!   passed, which is where an unrecoverable loss is counted.
//!
//! Three pieces, each from a cited source:
//!
//! - **The datagram.** A 16-byte header (magic `CL`, version, kind,
//!   `stream_tag`, `counter`) sent in the clear and authenticated as the
//!   associated data of ChaCha20-Poly1305 (RFC 8439,
//!   <https://www.rfc-editor.org/rfc/rfc8439>, read 2026-10-01), the cipher
//!   the session already uses (the vendored crate of ADR 0039). The nonce is
//!   the 4-byte `stream_tag` then the 8-byte `counter`, both big-endian: a
//!   key is fresh per offer and the counter never repeats under it, so no
//!   nonce is reused (RFC 8439 section 4: a nonce must not repeat under a key).
//! - **The replay window.** RFC 4303 section 3.4.3's sliding window
//!   (<https://www.rfc-editor.org/rfc/rfc4303#section-3.4.3>, read
//!   2026-10-01), 1024 counters wide, updated only after the tag verified.
//! - **The FEC.** One XOR parity per group of `k` data chunks, with the
//!   length recovery field of RFC 5109 section 7.3 and its zero-padding rule
//!   (section 9) (<https://www.rfc-editor.org/rfc/rfc5109.html>, read
//!   2026-10-01), and the optional column interleave of SMPTE ST 2022-1 as a
//!   vendor manual describes it
//!   (<https://portal.vbrick.com/doc/VB9000/490/H264_AdminGuide/8_Advanced.11.3.html>,
//!   read 2026-10-01): with depth `D` a group is every `D`-th chunk of a
//!   block of `k x D`, so a burst of up to `D` consecutive losses costs each
//!   group at most one chunk. Correctly received chunks are delivered at
//!   once (RFC 5109 section 15); only a repair waits for its parity.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use crate::message::{
    AudioChunk, Message as V1Message, SampleFormat, CHUNK_HEADER_LEN, RESERVED_LEN, RESERVED_OFFSET,
};
use crate::v2::catalog::{LOW_LATENCY_FEC_DEPTH, LOW_LATENCY_FEC_K, LOW_LATENCY_MAX_CHUNK_FRAMES};

/// The first two bytes of every datagram: ASCII `CL`.
pub const MAGIC: [u8; 2] = [0x43, 0x4C];

/// The datagram format's version.
pub const VERSION: u8 = 1;

/// Bytes in the clear header, which is also the AEAD's associated data.
pub const HEADER_LEN: usize = 16;

/// Bytes in a ChaCha20-Poly1305 tag.
pub const TAG_LEN: usize = 16;

/// Bytes in a stream key.
pub const KEY_LEN: usize = 32;

/// The largest datagram: 1500-byte Ethernet MTU minus the 20-byte IPv4 and
/// 8-byte UDP headers, so one datagram is one frame and never an IP fragment.
pub const MAX_DATAGRAM_LEN: usize = 1472;

/// The largest plaintext one datagram carries.
pub const MAX_PLAINTEXT_LEN: usize = MAX_DATAGRAM_LEN - HEADER_LEN - TAG_LEN;

/// The parity plaintext's own header: `group` u32, `fec_k` u8, `fec_depth`
/// u8, `length_xor` u16.
pub const PARITY_HEADER_LEN: usize = 8;

/// The largest data plaintext: the parity of a group is as long as its
/// longest chunk plus its 8-byte header, and it has to fit a datagram too.
pub const MAX_DATA_PLAINTEXT_LEN: usize = MAX_PLAINTEXT_LEN - PARITY_HEADER_LEN;

/// The smallest data plaintext: an `audio_chunk` payload with one PCM byte.
pub const MIN_DATA_PLAINTEXT_LEN: usize = CHUNK_HEADER_LEN + 1;

/// How many counters the replay window remembers behind the newest one.
pub const REPLAY_WINDOW: u64 = 1024;

/// `ll_marker`, the first byte of an audio chunk's reserved block on this
/// path. A chunk on the TCP path keeps the block zero (ADR 0004), so a 1 here
/// is what says the other four fields mean something.
pub const LL_MARKER: u8 = 1;

/// How many blocks of `k x depth` chunks a decoder keeps open: a group is
/// closed (its losses counted, its late packets dropped) once a packet two
/// blocks newer arrived. ASSUMED: at the defaults a block is 10 ms, so a
/// packet may arrive up to about 20 ms out of order and still count, which is
/// the whole of the playout budget.
pub const OPEN_BLOCKS: u32 = 3;

/// What a datagram carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// An `audio_chunk` payload, its reserved block assigned.
    Data,
    /// The XOR parity of one group.
    Parity,
}

impl Kind {
    /// The wire byte.
    pub fn to_wire(self) -> u8 {
        match self {
            Kind::Data => 1,
            Kind::Parity => 2,
        }
    }

    /// The kind for a wire byte, or `None` when it is undefined.
    pub fn from_wire(byte: u8) -> Option<Kind> {
        match byte {
            1 => Some(Kind::Data),
            2 => Some(Kind::Parity),
            _ => None,
        }
    }

    /// Short stable name, used by fixtures and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Data => "data",
            Kind::Parity => "parity",
        }
    }
}

/// Why a datagram was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowLatError {
    /// A plaintext longer than its datagram can carry.
    TooLong {
        /// Bytes it had.
        len: usize,
        /// The most allowed.
        max: usize,
    },
    /// A data plaintext shorter than an audio chunk with one PCM byte.
    TooShort {
        /// Bytes it had.
        len: usize,
    },
    /// `stream_tag` 0 names no stream.
    ZeroStreamTag,
    /// The key is all zero.
    ZeroKey,
    /// FEC parameters outside the offer's rules.
    BadParams(&'static str),
    /// The counter reached `u64::MAX`; the stream needs a new offer.
    CounterExhausted,
    /// The group number would pass `u32::MAX`; the stream needs a new offer.
    GroupsExhausted,
}

impl fmt::Display for LowLatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LowLatError::TooLong { len, max } => {
                write!(f, "a {} byte plaintext exceeds the {} allowed", len, max)
            }
            LowLatError::TooShort { len } => write!(
                f,
                "a {} byte data plaintext is shorter than an audio chunk ({})",
                len, MIN_DATA_PLAINTEXT_LEN
            ),
            LowLatError::ZeroStreamTag => write!(f, "stream_tag 0 names no stream"),
            LowLatError::ZeroKey => write!(f, "a stream key is never all zero"),
            LowLatError::BadParams(why) => write!(f, "FEC parameters: {}", why),
            LowLatError::CounterExhausted => write!(f, "the datagram counter is spent"),
            LowLatError::GroupsExhausted => write!(f, "the FEC group number is spent"),
        }
    }
}

impl std::error::Error for LowLatError {}

/// Why a received datagram was dropped. Each drops one datagram and nothing
/// else: a UDP receiver never closes on a bad packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenError {
    /// Shorter than a header and a tag.
    TooShort,
    /// Longer than [`MAX_DATAGRAM_LEN`].
    TooLong,
    /// The first two bytes are not `CL`.
    BadMagic,
    /// A version this side does not speak.
    BadVersion,
    /// A kind byte with no meaning.
    BadKind,
    /// A `stream_tag` other than the one this receiver was offered.
    WrongStreamTag,
    /// A counter already seen, or older than the replay window.
    Replayed,
    /// The tag did not verify: a wrong key, or altered bytes.
    AuthFailed,
}

impl OpenError {
    /// Short stable name, used by fixtures and diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            OpenError::TooShort => "too_short",
            OpenError::TooLong => "too_long",
            OpenError::BadMagic => "bad_magic",
            OpenError::BadVersion => "bad_version",
            OpenError::BadKind => "bad_kind",
            OpenError::WrongStreamTag => "wrong_stream_tag",
            OpenError::Replayed => "replayed",
            OpenError::AuthFailed => "auth_failed",
        }
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "datagram dropped: {}", self.name())
    }
}

impl std::error::Error for OpenError {}

/// The clear header of a datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Data or parity.
    pub kind: Kind,
    /// The stream, as the offer named it.
    pub stream_tag: u32,
    /// Strictly increasing per sender per stream; never reused under a key.
    pub counter: u64,
}

impl Header {
    /// The 16 header bytes.
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut b = [0u8; HEADER_LEN];
        b[0..2].copy_from_slice(&MAGIC);
        b[2] = VERSION;
        b[3] = self.kind.to_wire();
        b[4..8].copy_from_slice(&self.stream_tag.to_be_bytes());
        b[8..16].copy_from_slice(&self.counter.to_be_bytes());
        b
    }

    /// Parse the header at the front of a datagram, checking its length,
    /// magic, version and kind (in that order).
    pub fn parse(datagram: &[u8]) -> Result<Header, OpenError> {
        if datagram.len() < HEADER_LEN + TAG_LEN {
            return Err(OpenError::TooShort);
        }
        if datagram.len() > MAX_DATAGRAM_LEN {
            return Err(OpenError::TooLong);
        }
        if datagram[0..2] != MAGIC {
            return Err(OpenError::BadMagic);
        }
        if datagram[2] != VERSION {
            return Err(OpenError::BadVersion);
        }
        let kind = Kind::from_wire(datagram[3]).ok_or(OpenError::BadKind)?;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&datagram[4..8]);
        let mut counter = [0u8; 8];
        counter.copy_from_slice(&datagram[8..16]);
        Ok(Header {
            kind,
            stream_tag: u32::from_be_bytes(tag),
            counter: u64::from_be_bytes(counter),
        })
    }
}

/// The AEAD nonce of a datagram: `stream_tag` then `counter`, big-endian.
pub fn nonce(stream_tag: u32, counter: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[0..4].copy_from_slice(&stream_tag.to_be_bytes());
    n[4..12].copy_from_slice(&counter.to_be_bytes());
    n
}

/// Seal one datagram: the header, then the plaintext encrypted under `key`
/// with the header as associated data, then the tag.
pub fn seal(
    key: &[u8; KEY_LEN],
    header: &Header,
    plaintext: &[u8],
) -> Result<Vec<u8>, LowLatError> {
    if header.stream_tag == 0 {
        return Err(LowLatError::ZeroStreamTag);
    }
    if plaintext.len() > MAX_PLAINTEXT_LEN {
        return Err(LowLatError::TooLong {
            len: plaintext.len(),
            max: MAX_PLAINTEXT_LEN,
        });
    }
    let head = header.to_bytes();
    let cipher = ChaCha20Poly1305::new(&Key::from(*key));
    let body = cipher
        .encrypt(
            &Nonce::from(nonce(header.stream_tag, header.counter)),
            Payload {
                msg: plaintext,
                aad: &head,
            },
        )
        .map_err(|_| LowLatError::TooLong {
            len: plaintext.len(),
            max: MAX_PLAINTEXT_LEN,
        })?;
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&head);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Open one datagram without a replay window: the header's checks, then the
/// tag. [`Opener`] adds the stream's tag and the window.
pub fn open(key: &[u8; KEY_LEN], datagram: &[u8]) -> Result<(Header, Vec<u8>), OpenError> {
    let header = Header::parse(datagram)?;
    let cipher = ChaCha20Poly1305::new(&Key::from(*key));
    let plaintext = cipher
        .decrypt(
            &Nonce::from(nonce(header.stream_tag, header.counter)),
            Payload {
                msg: &datagram[HEADER_LEN..],
                aad: &datagram[..HEADER_LEN],
            },
        )
        .map_err(|_| OpenError::AuthFailed)?;
    Ok((header, plaintext))
}

/// RFC 4303 section 3.4.3's anti-replay window, [`REPLAY_WINDOW`] counters
/// wide: the newest counter seen, and a bit per counter behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayWindow {
    top: u64,
    any: bool,
    // Bit `a` (word a / 64, bit a % 64) is "counter top - a was seen".
    bits: [u64; (REPLAY_WINDOW / 64) as usize],
}

impl Default for ReplayWindow {
    fn default() -> Self {
        ReplayWindow::new()
    }
}

impl ReplayWindow {
    /// An empty window: the first counter of any value is accepted.
    pub fn new() -> ReplayWindow {
        ReplayWindow {
            top: 0,
            any: false,
            bits: [0; (REPLAY_WINDOW / 64) as usize],
        }
    }

    /// Whether `counter` would be accepted: newer than the newest, or inside
    /// the window and not yet seen.
    pub fn accepts(&self, counter: u64) -> bool {
        if !self.any || counter > self.top {
            return true;
        }
        let age = self.top - counter;
        if age >= REPLAY_WINDOW {
            return false;
        }
        self.bits[(age / 64) as usize] & (1u64 << (age % 64)) == 0
    }

    /// Record `counter` as seen. Called only after its tag verified, so a
    /// forged packet cannot move the window (RFC 4303 section 3.4.3).
    pub fn commit(&mut self, counter: u64) {
        if !self.any {
            self.any = true;
            self.top = counter;
            self.bits = [0; (REPLAY_WINDOW / 64) as usize];
        } else if counter > self.top {
            self.shift(counter - self.top);
            self.top = counter;
        }
        let age = self.top - counter;
        if age < REPLAY_WINDOW {
            self.bits[(age / 64) as usize] |= 1u64 << (age % 64);
        }
    }

    fn shift(&mut self, by: u64) {
        let n = self.bits.len();
        if by >= REPLAY_WINDOW {
            self.bits = [0; (REPLAY_WINDOW / 64) as usize];
            return;
        }
        let words = (by / 64) as usize;
        let bits = (by % 64) as u32;
        for i in (0..n).rev() {
            let mut v = 0u64;
            if i >= words {
                v = self.bits[i - words] << bits;
                if bits > 0 && i > words {
                    v |= self.bits[i - words - 1] >> (64 - bits);
                }
            }
            self.bits[i] = v;
        }
    }
}

/// A sender's half of one stream's datagrams: the key, the tag and the
/// counter, which starts at 0 and only ever goes up.
#[derive(Debug, Clone)]
pub struct Sealer {
    key: [u8; KEY_LEN],
    stream_tag: u32,
    next_counter: u64,
}

impl Sealer {
    /// A sealer for the stream an offer named.
    pub fn new(key: [u8; KEY_LEN], stream_tag: u32) -> Result<Sealer, LowLatError> {
        Sealer::starting_at(key, stream_tag, 0)
    }

    /// A sealer whose first counter is `counter` (the vectors use it).
    pub fn starting_at(
        key: [u8; KEY_LEN],
        stream_tag: u32,
        counter: u64,
    ) -> Result<Sealer, LowLatError> {
        if stream_tag == 0 {
            return Err(LowLatError::ZeroStreamTag);
        }
        if key == [0u8; KEY_LEN] {
            return Err(LowLatError::ZeroKey);
        }
        Ok(Sealer {
            key,
            stream_tag,
            next_counter: counter,
        })
    }

    /// The counter the next datagram gets.
    pub fn next_counter(&self) -> u64 {
        self.next_counter
    }

    /// Seal one datagram with the next counter.
    pub fn seal(&mut self, datagram: &Datagram) -> Result<Vec<u8>, LowLatError> {
        if self.next_counter == u64::MAX {
            return Err(LowLatError::CounterExhausted);
        }
        let header = Header {
            kind: datagram.kind,
            stream_tag: self.stream_tag,
            counter: self.next_counter,
        };
        let out = seal(&self.key, &header, &datagram.plaintext)?;
        self.next_counter += 1;
        Ok(out)
    }
}

/// What a receiver dropped, by reason, and what it opened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenStats {
    /// Datagrams that authenticated and were new.
    pub opened: u64,
    /// Too short, too long, a bad magic, version or kind.
    pub malformed: u64,
    /// Another stream's tag.
    pub wrong_stream_tag: u64,
    /// A counter already seen or older than the window.
    pub replayed: u64,
    /// The tag did not verify.
    pub auth_failed: u64,
}

/// One datagram that authenticated and was new.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// Data or parity.
    pub kind: Kind,
    /// Its counter.
    pub counter: u64,
    /// The plaintext, for [`FecDecoder::push`].
    pub plaintext: Vec<u8>,
}

/// A receiver's half of one stream's datagrams: the key, the tag it was
/// offered, the replay window and the counts.
#[derive(Debug, Clone)]
pub struct Opener {
    key: [u8; KEY_LEN],
    stream_tag: u32,
    window: ReplayWindow,
    stats: OpenStats,
}

impl Opener {
    /// A receiver for the stream an offer named.
    pub fn new(key: [u8; KEY_LEN], stream_tag: u32) -> Opener {
        Opener {
            key,
            stream_tag,
            window: ReplayWindow::new(),
            stats: OpenStats::default(),
        }
    }

    /// The counts so far.
    pub fn stats(&self) -> OpenStats {
        self.stats
    }

    /// Open one datagram. In order: the header's form, the stream's tag, the
    /// replay window (cheap, before any cryptography), the AEAD tag, and only
    /// then the window is moved.
    pub fn open(&mut self, datagram: &[u8]) -> Result<Opened, OpenError> {
        let result = self.open_inner(datagram);
        match result {
            Ok(_) => self.stats.opened += 1,
            Err(OpenError::WrongStreamTag) => self.stats.wrong_stream_tag += 1,
            Err(OpenError::Replayed) => self.stats.replayed += 1,
            Err(OpenError::AuthFailed) => self.stats.auth_failed += 1,
            Err(_) => self.stats.malformed += 1,
        }
        result
    }

    fn open_inner(&mut self, datagram: &[u8]) -> Result<Opened, OpenError> {
        let header = Header::parse(datagram)?;
        if header.stream_tag != self.stream_tag {
            return Err(OpenError::WrongStreamTag);
        }
        if !self.window.accepts(header.counter) {
            return Err(OpenError::Replayed);
        }
        let (header, plaintext) = open(&self.key, datagram)?;
        self.window.commit(header.counter);
        Ok(Opened {
            kind: header.kind,
            counter: header.counter,
            plaintext,
        })
    }
}

/// The reserved block of an audio chunk on this path (ADR 0004's 14 bytes at
/// offset 18): offset 18 `ll_marker` (1), 19 `fec_k`, 20 `fec_depth`, 21
/// `group_index`, 22..26 `group` (u32 big-endian), 26..32 zero. The TCP path
/// keeps writing zeros and every decoder still ignores the block there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkInfo {
    /// The stream's `fec_k`.
    pub fec_k: u8,
    /// The stream's `fec_depth`.
    pub fec_depth: u8,
    /// This chunk's position in its group, 0 to k - 1 (0 without FEC).
    pub group_index: u8,
    /// Its group's number (its chunk number without FEC).
    pub group: u32,
}

impl ChunkInfo {
    /// The 14 reserved bytes for this chunk.
    pub fn to_reserved(&self) -> [u8; RESERVED_LEN] {
        let mut r = [0u8; RESERVED_LEN];
        r[0] = LL_MARKER;
        r[1] = self.fec_k;
        r[2] = self.fec_depth;
        r[3] = self.group_index;
        r[4..8].copy_from_slice(&self.group.to_be_bytes());
        r
    }

    /// The info in a reserved block, or `None` when the block does not carry
    /// the marker (a TCP chunk) or the trailing six bytes are not zero.
    pub fn from_reserved(reserved: &[u8]) -> Option<ChunkInfo> {
        if reserved.len() != RESERVED_LEN || reserved[0] != LL_MARKER {
            return None;
        }
        if reserved[8..].iter().any(|&b| b != 0) {
            return None;
        }
        let mut g = [0u8; 4];
        g.copy_from_slice(&reserved[4..8]);
        Some(ChunkInfo {
            fec_k: reserved[1],
            fec_depth: reserved[2],
            group_index: reserved[3],
            group: u32::from_be_bytes(g),
        })
    }

    /// The info in a data plaintext (an `audio_chunk` payload).
    pub fn from_payload(payload: &[u8]) -> Option<ChunkInfo> {
        payload
            .get(RESERVED_OFFSET..RESERVED_OFFSET + RESERVED_LEN)
            .and_then(ChunkInfo::from_reserved)
    }
}

/// Whether a chunk of `chunk_frames` frames of this layout fits one data
/// datagram ([`MAX_DATA_PLAINTEXT_LEN`]). An offer whose chunk would not fit
/// is refused when it is built, never split.
pub fn chunk_fits(chunk_frames: u32, channels: u16, format: SampleFormat) -> bool {
    let pcm = chunk_frames as u64 * channels as u64 * format.bytes_per_sample() as u64;
    chunk_frames >= 1
        && chunk_frames <= LOW_LATENCY_MAX_CHUNK_FRAMES
        && CHUNK_HEADER_LEN as u64 + pcm <= MAX_DATA_PLAINTEXT_LEN as u64
}

/// A stream's FEC shape: `k` data chunks per parity (0 for none) and the
/// column-interleave depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FecParams {
    k: u8,
    depth: u8,
}

impl FecParams {
    /// The offer's `fec_k` and `fec_depth`, checked by the offer's rules.
    pub fn new(k: u8, depth: u8) -> Result<FecParams, LowLatError> {
        if k != 0 && (k < LOW_LATENCY_FEC_K.0 || k > LOW_LATENCY_FEC_K.1) {
            return Err(LowLatError::BadParams("fec_k is 0 or 2 to 16"));
        }
        if depth < LOW_LATENCY_FEC_DEPTH.0 || depth > LOW_LATENCY_FEC_DEPTH.1 {
            return Err(LowLatError::BadParams("fec_depth is 1 to 8"));
        }
        if k == 0 && depth != 1 {
            return Err(LowLatError::BadParams(
                "a stream without FEC has no interleave",
            ));
        }
        Ok(FecParams { k, depth })
    }

    /// No FEC: every chunk is its own group and no parity is sent.
    pub fn none() -> FecParams {
        FecParams { k: 0, depth: 1 }
    }

    /// Data chunks per parity; 0 for none.
    pub fn k(&self) -> u8 {
        self.k
    }

    /// The interleave depth.
    pub fn depth(&self) -> u8 {
        self.depth
    }

    /// Chunks in one group: `k`, or 1 without FEC.
    pub fn group_len(&self) -> u32 {
        u32::from(self.k.max(1))
    }

    /// Chunks in one block: `group_len x depth`.
    pub fn block_len(&self) -> u64 {
        u64::from(self.group_len()) * u64::from(self.depth)
    }

    /// The group and position of chunk `n` (counted from 0 in the stream),
    /// or `None` past the last group number.
    pub fn locate(&self, n: u64) -> Option<(u32, u8)> {
        let block = n / self.block_len();
        let offset = n % self.block_len();
        let column = offset % u64::from(self.depth);
        let index = offset / u64::from(self.depth);
        let group = block * u64::from(self.depth) + column;
        u32::try_from(group).ok().map(|g| (g, index as u8))
    }

    /// The chunk number of position `index` of `group`.
    pub fn chunk_index(&self, group: u32, index: u8) -> u64 {
        let depth = u64::from(self.depth);
        let block = u64::from(group) / depth;
        let column = u64::from(group) % depth;
        block * self.block_len() + column + u64::from(index) * depth
    }

    /// How many chunk durations the worst-placed lost chunk waits for its
    /// repair: the first chunk of a group is repaired when the parity sent
    /// right after its last chunk arrives, `(k - 1) x depth` chunks later.
    /// 0 without FEC.
    pub fn wait_chunks(&self) -> u64 {
        if self.k == 0 {
            0
        } else {
            u64::from(self.k - 1) * u64::from(self.depth)
        }
    }
}

/// One datagram's plaintext, before it is sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    /// Data or parity.
    pub kind: Kind,
    /// The bytes [`Sealer::seal`] encrypts.
    pub plaintext: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
struct Column {
    xor: Vec<u8>,
    len_xor: u16,
    count: u32,
}

fn xor_into(acc: &mut Vec<u8>, bytes: &[u8]) {
    if acc.len() < bytes.len() {
        acc.resize(bytes.len(), 0);
    }
    for (a, b) in acc.iter_mut().zip(bytes) {
        *a ^= b;
    }
}

/// The sender's FEC: numbers the chunks, writes each one's [`ChunkInfo`],
/// and emits a group's parity right after the group's last chunk.
#[derive(Debug, Clone)]
pub struct FecEncoder {
    params: FecParams,
    next: u64,
    columns: Vec<Column>,
}

impl FecEncoder {
    /// An encoder for a new stream; its first chunk is chunk 0.
    pub fn new(params: FecParams) -> FecEncoder {
        FecEncoder {
            params,
            next: 0,
            columns: vec![Column::default(); usize::from(params.depth)],
        }
    }

    /// The stream's FEC shape.
    pub fn params(&self) -> FecParams {
        self.params
    }

    /// Chunks pushed so far.
    pub fn chunks(&self) -> u64 {
        self.next
    }

    /// Push one chunk: its reserved block is assigned and it is encoded as an
    /// `audio_chunk` payload, then sent as [`FecEncoder::push_payload`] sends.
    pub fn push(&mut self, chunk: &AudioChunk) -> Result<Vec<Datagram>, LowLatError> {
        let mut c = chunk.clone();
        c.reserved = [0; RESERVED_LEN];
        let mut payload = crate::codec::encode_payload(&V1Message::AudioChunk(c))
            .map_err(|_| LowLatError::TooShort { len: 0 })?;
        self.push_payload(&mut payload)
    }

    /// Push one `audio_chunk` payload: bytes 18..32 are overwritten with this
    /// chunk's [`ChunkInfo`], and the datagrams to send come back in order
    /// (the data, then the parity when this chunk completed its group).
    pub fn push_payload(&mut self, payload: &mut [u8]) -> Result<Vec<Datagram>, LowLatError> {
        if payload.len() < MIN_DATA_PLAINTEXT_LEN {
            return Err(LowLatError::TooShort { len: payload.len() });
        }
        if payload.len() > MAX_DATA_PLAINTEXT_LEN {
            return Err(LowLatError::TooLong {
                len: payload.len(),
                max: MAX_DATA_PLAINTEXT_LEN,
            });
        }
        let (group, index) = self
            .params
            .locate(self.next)
            .ok_or(LowLatError::GroupsExhausted)?;
        let info = ChunkInfo {
            fec_k: self.params.k,
            fec_depth: self.params.depth,
            group_index: index,
            group,
        };
        payload[RESERVED_OFFSET..RESERVED_OFFSET + RESERVED_LEN]
            .copy_from_slice(&info.to_reserved());
        self.next += 1;
        let mut out = vec![Datagram {
            kind: Kind::Data,
            plaintext: payload.to_vec(),
        }];
        if self.params.k == 0 {
            return Ok(out);
        }
        let col = &mut self.columns[(group % u32::from(self.params.depth)) as usize];
        xor_into(&mut col.xor, payload);
        col.len_xor ^= payload.len() as u16;
        col.count += 1;
        if col.count == u32::from(self.params.k) {
            let mut parity = Vec::with_capacity(PARITY_HEADER_LEN + col.xor.len());
            parity.extend_from_slice(&group.to_be_bytes());
            parity.push(self.params.k);
            parity.push(self.params.depth);
            parity.extend_from_slice(&col.len_xor.to_be_bytes());
            parity.extend_from_slice(&col.xor);
            *col = Column::default();
            out.push(Datagram {
                kind: Kind::Parity,
                plaintext: parity,
            });
        }
        Ok(out)
    }
}

/// One chunk a decoder hands on: received, or rebuilt from its group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivered {
    /// Its number in the stream, counted from 0.
    pub chunk_index: u64,
    /// Its group.
    pub group: u32,
    /// Its position in the group.
    pub group_index: u8,
    /// Whether it was rebuilt from the parity rather than received.
    pub recovered: bool,
    /// The `audio_chunk` payload, its reserved block as sent.
    pub payload: Vec<u8>,
}

/// What a decoder saw and did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FecStats {
    /// Data datagrams accepted.
    pub data: u64,
    /// Parity datagrams accepted.
    pub parity: u64,
    /// Chunks handed on, received or rebuilt.
    pub delivered: u64,
    /// Chunks rebuilt from a parity.
    pub recovered: u64,
    /// Chunks of a closed group that never arrived and could not be rebuilt.
    pub unrecoverable: u64,
    /// Datagrams for a group already closed.
    pub late: u64,
    /// A chunk or a parity this decoder already had.
    pub duplicate: u64,
    /// A plaintext whose FEC fields contradict the stream's, or a rebuild
    /// that does not carry its own group's fields.
    pub rejected: u64,
}

#[derive(Debug, Clone, Default)]
struct Slot {
    used: bool,
    group: u32,
    have: u32,
    parity: bool,
    failed: bool,
    acc: Vec<u8>,
    len_xor: u16,
}

/// The receiver's FEC: hands on every chunk as it arrives, rebuilds the one
/// missing chunk of a group once the group's parity and the other `k - 1`
/// are in, and counts what it could not.
#[derive(Debug, Clone)]
pub struct FecDecoder {
    params: FecParams,
    slots: Vec<Slot>,
    started: bool,
    next_close: u32,
    newest_block: u32,
    stats: FecStats,
}

impl FecDecoder {
    /// A decoder for the FEC shape an offer named.
    pub fn new(params: FecParams) -> FecDecoder {
        FecDecoder {
            params,
            slots: vec![Slot::default(); (OPEN_BLOCKS * u32::from(params.depth)) as usize],
            started: false,
            next_close: 0,
            newest_block: 0,
            stats: FecStats::default(),
        }
    }

    /// The counts so far.
    pub fn stats(&self) -> FecStats {
        self.stats
    }

    /// The first group not yet closed.
    pub fn next_open_group(&self) -> u32 {
        self.next_close
    }

    /// Push one opened datagram's plaintext; returns the chunks it makes
    /// available, in the order they became so.
    pub fn push(&mut self, kind: Kind, plaintext: &[u8]) -> Vec<Delivered> {
        let depth = u32::from(self.params.depth);
        let (group, data_index) = match kind {
            Kind::Data => match self.data_fields(plaintext) {
                Some(info) => (info.group, Some(info.group_index)),
                None => {
                    self.stats.rejected += 1;
                    return Vec::new();
                }
            },
            Kind::Parity => match self.parity_group(plaintext) {
                Some(g) => (g, None),
                None => {
                    self.stats.rejected += 1;
                    return Vec::new();
                }
            },
        };
        let block = group / depth;
        if !self.started {
            self.started = true;
            self.next_close = block * depth;
            self.newest_block = block;
        }
        if group < self.next_close {
            self.stats.late += 1;
            return Vec::new();
        }
        if block > self.newest_block {
            self.newest_block = block;
            if let Some(first_open) = block.checked_sub(OPEN_BLOCKS - 1) {
                self.close_through(first_open.saturating_mul(depth), u64::MAX);
            }
        }
        let n = self.slots.len();
        let slot = &mut self.slots[group as usize % n];
        if !slot.used || slot.group != group {
            *slot = Slot {
                used: true,
                group,
                ..Slot::default()
            };
        }
        let mut out = Vec::new();
        match data_index {
            Some(index) => {
                let bit = 1u32 << index;
                if slot.have & bit != 0 {
                    self.stats.duplicate += 1;
                    return out;
                }
                self.stats.data += 1;
                xor_into(&mut slot.acc, plaintext);
                slot.len_xor ^= plaintext.len() as u16;
                slot.have |= bit;
                self.stats.delivered += 1;
                out.push(Delivered {
                    chunk_index: self.params.chunk_index(group, index),
                    group,
                    group_index: index,
                    recovered: false,
                    payload: plaintext.to_vec(),
                });
            }
            None => {
                if slot.parity {
                    self.stats.duplicate += 1;
                    return out;
                }
                self.stats.parity += 1;
                let mut lx = [0u8; 2];
                lx.copy_from_slice(&plaintext[6..8]);
                xor_into(&mut slot.acc, &plaintext[PARITY_HEADER_LEN..]);
                slot.len_xor ^= u16::from_be_bytes(lx);
                slot.parity = true;
            }
        }
        if let Some(d) = self.try_recover(group) {
            out.push(d);
        }
        out
    }

    /// Close every group before `group`: the playout point has passed them,
    /// so what they still miss is lost and what still arrives for them is
    /// late.
    pub fn expire_before(&mut self, group: u32) {
        if self.started {
            self.close_through(group, u64::MAX);
        }
    }

    /// The stream ended after `chunks_sent` chunks (from its `stream_end`):
    /// close every group, counting as lost only chunks that were sent. The
    /// chunks of a last, incomplete group had no parity and are not counted
    /// as anything but received or lost.
    pub fn finish(&mut self, chunks_sent: u64) {
        if !self.started || chunks_sent == 0 {
            return;
        }
        let last = chunks_sent - 1;
        let Some((last_group, _)) = self.params.locate(last) else {
            return;
        };
        let depth = u32::from(self.params.depth);
        let end = (last_group / depth + 1).saturating_mul(depth);
        self.close_through(end, chunks_sent);
    }

    fn data_fields(&self, plaintext: &[u8]) -> Option<ChunkInfo> {
        if plaintext.len() < MIN_DATA_PLAINTEXT_LEN || plaintext.len() > MAX_DATA_PLAINTEXT_LEN {
            return None;
        }
        let info = ChunkInfo::from_payload(plaintext)?;
        let ok = info.fec_k == self.params.k
            && info.fec_depth == self.params.depth
            && u32::from(info.group_index) < self.params.group_len();
        ok.then_some(info)
    }

    fn parity_group(&self, plaintext: &[u8]) -> Option<u32> {
        if self.params.k == 0
            || plaintext.len() < PARITY_HEADER_LEN + MIN_DATA_PLAINTEXT_LEN
            || plaintext.len() > MAX_PLAINTEXT_LEN
            || plaintext[4] != self.params.k
            || plaintext[5] != self.params.depth
        {
            return None;
        }
        let mut g = [0u8; 4];
        g.copy_from_slice(&plaintext[0..4]);
        Some(u32::from_be_bytes(g))
    }

    fn try_recover(&mut self, group: u32) -> Option<Delivered> {
        let k = self.params.group_len();
        let n = self.slots.len();
        let slot = &mut self.slots[group as usize % n];
        if self.params.k == 0 || !slot.parity || slot.failed || slot.have.count_ones() != k - 1 {
            return None;
        }
        let missing = (!slot.have & ((1u32 << k) - 1)).trailing_zeros() as u8;
        let len = usize::from(slot.len_xor);
        let fields_ok = len >= MIN_DATA_PLAINTEXT_LEN
            && len <= slot.acc.len()
            && ChunkInfo::from_payload(&slot.acc[..len])
                == Some(ChunkInfo {
                    fec_k: self.params.k,
                    fec_depth: self.params.depth,
                    group_index: missing,
                    group,
                });
        if !fields_ok {
            // Not this group's chunk: a sender bug, never a guess played.
            slot.failed = true;
            self.stats.rejected += 1;
            return None;
        }
        slot.have |= 1u32 << missing;
        let payload = slot.acc[..len].to_vec();
        self.stats.recovered += 1;
        self.stats.delivered += 1;
        Some(Delivered {
            chunk_index: self.params.chunk_index(group, missing),
            group,
            group_index: missing,
            recovered: true,
            payload,
        })
    }

    /// Close groups `next_close..limit`, counting each one's chunks below
    /// `chunks_sent` that it never had.
    fn close_through(&mut self, limit: u32, chunks_sent: u64) {
        if limit <= self.next_close {
            return;
        }
        let n = self.slots.len() as u32;
        let k = self.params.group_len();
        let held_end = limit.min(self.next_close.saturating_add(n));
        for g in self.next_close..held_end {
            let slot = &mut self.slots[(g % n) as usize];
            let have = if slot.used && slot.group == g {
                slot.have
            } else {
                0
            };
            for i in 0..k {
                if have & (1u32 << i) == 0 && self.params.chunk_index(g, i as u8) < chunks_sent {
                    self.stats.unrecoverable += 1;
                }
            }
            if slot.used && slot.group == g {
                *slot = Slot::default();
            }
        }
        // A jump past every held slot: those groups had no packet at all.
        if limit > held_end {
            self.stats.unrecoverable += u64::from(limit - held_end) * u64::from(k);
        }
        self.next_close = limit;
    }
}

/// Where a budget figure comes from (BRIEF.md section 3.1 rule 3: a number
/// not measured or cited is ASSUMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A published figure: the citation, with the date it was read.
    Cited(&'static str),
    /// Arithmetic from other figures: the formula.
    Computed(&'static str),
    /// Chosen, not measured: why, and what would replace it.
    Assumed(&'static str),
}

impl Source {
    /// `cited`, `computed` or `ASSUMED`.
    pub fn label(&self) -> &'static str {
        match self {
            Source::Cited(_) => "cited",
            Source::Computed(_) => "computed",
            Source::Assumed(_) => "ASSUMED",
        }
    }

    /// The citation, formula or reason.
    pub fn text(&self) -> &'static str {
        match self {
            Source::Cited(t) | Source::Computed(t) | Source::Assumed(t) => t,
        }
    }
}

/// One stage of the capture-to-air budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetItem {
    /// The stage, as the report names it.
    pub stage: &'static str,
    /// Its worst-case share, in ns.
    pub ns: u64,
    /// Where the figure comes from.
    pub source: Source,
}

/// Why a stamp lead is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatencyError {
    /// Outside the configurable range.
    OutOfRange {
        /// What was asked for.
        asked_ns: u64,
    },
    /// Below the floor: a lost chunk's repair (or an on-time chunk) would
    /// arrive after its playout point.
    BelowFloor {
        /// What was asked for.
        asked_ns: u64,
        /// The floor for this plan.
        floor_ns: u64,
    },
}

impl fmt::Display for LatencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LatencyError::OutOfRange { asked_ns } => write!(
                f,
                "a TV latency of {} ns is outside {} to {} ns",
                asked_ns, DEFAULTS.l_tv_range_ns.0, DEFAULTS.l_tv_range_ns.1
            ),
            LatencyError::BelowFloor { asked_ns, floor_ns } => write!(
                f,
                "a TV latency of {} ns is below this plan's floor of {} ns",
                asked_ns, floor_ns
            ),
        }
    }
}

impl std::error::Error for LatencyError {}

/// The low-latency path's settings and its latency budget: what the server,
/// the hub and the client use unless a flag overrides it, and what the
/// simulator (`crates/sync/src/lowlat_sim.rs`) runs. Each figure's source is
/// in [`Plan::budget`] and the decision record; the simulation report
/// (`docs/measurements/low-latency-budget-sim.md`) is not timing evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// The stream's sample rate.
    pub sample_rate_hz: u32,
    /// PCM frames per chunk.
    pub chunk_frames: u32,
    /// Data chunks per parity; 0 for none.
    pub fec_k: u8,
    /// Column-interleave depth; 1 for none.
    pub fec_depth: u8,
    /// `L_tv`: from the capture stamp to the speaker, in ns.
    pub l_tv_ns: u64,
    /// The range `L_tv` may be configured in, inclusive, in ns.
    pub l_tv_range_ns: (u64, u64),
    /// The hub's capture buffering past the stamp, in ns.
    pub capture_ns: u64,
    /// One UDP leg: send, the switch hops, receive, in ns.
    pub network_leg_ns: u64,
    /// The server's relay: receive, restamp, send, in ns.
    pub relay_ns: u64,
    /// Margin for jitter and scheduling on top of the above, in ns.
    pub jitter_margin_ns: u64,
    /// The endpoint's output path: its buffer and DSP block, in ns.
    pub endpoint_output_ns: u64,
    /// The DAC's digital filter, in ns.
    pub dac_filter_ns: u64,
    /// The S/PDIF receiver in front of the hub's capture, in ns. Before the
    /// stamp, so not in the floor; it counts against lip sync.
    pub spdif_receiver_ns: u64,
}

/// The defaults (goal 13). Changing one changes the server's, the hub's and
/// the client's behaviour together, and the simulation report with them.
pub const DEFAULTS: Plan = Plan {
    sample_rate_hz: 48_000,
    // 2.5 ms: the research's proposal (fec-latency.md section 6), one Opus
    // frame duration at 48 kHz (RFC 6716), 400 datagrams a second.
    chunk_frames: 120,
    fec_k: 4,
    fec_depth: 1,
    l_tv_ns: 20_000_000,
    l_tv_range_ns: (10_000_000, 40_000_000),
    capture_ns: 2_000_000,
    network_leg_ns: 250_000,
    relay_ns: 500_000,
    jitter_margin_ns: 2_000_000,
    endpoint_output_ns: 4_000_000,
    // 20 / 48000 s: the PCM5102A's normal 8x filter latency, 20 tS.
    dac_filter_ns: 416_667,
    // 3 / 48000 s: the DIR9001's output latency, 3/fS.
    spdif_receiver_ns: 62_500,
};

impl Plan {
    /// One chunk's duration, in ns (rounded down).
    pub fn chunk_ns(&self) -> u64 {
        u64::from(self.chunk_frames) * 1_000_000_000 / u64::from(self.sample_rate_hz.max(1))
    }

    /// The FEC shape this plan names.
    pub fn fec(&self) -> Result<FecParams, LowLatError> {
        FecParams::new(self.fec_k, self.fec_depth)
    }

    /// The worst-placed lost chunk's wait for its repair, in ns:
    /// `(k - 1) x depth` chunk durations (0 without FEC).
    pub fn fec_wait_ns(&self) -> u64 {
        let waits = if self.fec_k == 0 {
            0
        } else {
            u64::from(self.fec_k - 1) * u64::from(self.fec_depth)
        };
        waits * self.chunk_ns()
    }

    /// The floor rule: the least `L_tv` under which every chunk, including
    /// the worst-placed repaired one, reaches the endpoint's output path in
    /// time. Capture, one chunk to fill, the FEC wait, both UDP legs, the
    /// relay, the jitter margin, the endpoint's output path and the DAC.
    pub fn floor_ns(&self) -> u64 {
        self.budget().iter().map(|i| i.ns).sum()
    }

    /// Whether `l_tv_ns` may be used: inside the range and not below the
    /// floor. A value below the floor is refused, never quietly raised.
    pub fn check_latency(&self, l_tv_ns: u64) -> Result<(), LatencyError> {
        if l_tv_ns < self.l_tv_range_ns.0 || l_tv_ns > self.l_tv_range_ns.1 {
            return Err(LatencyError::OutOfRange { asked_ns: l_tv_ns });
        }
        let floor_ns = self.floor_ns();
        if l_tv_ns < floor_ns {
            return Err(LatencyError::BelowFloor {
                asked_ns: l_tv_ns,
                floor_ns,
            });
        }
        Ok(())
    }

    /// The stages from the capture stamp to the speaker, each with its
    /// source. Their sum is [`Plan::floor_ns`].
    pub fn budget(&self) -> [BudgetItem; 9] {
        [
            BudgetItem {
                stage: "capture buffering (two ALSA periods of 1 ms)",
                ns: self.capture_ns,
                source: Source::Assumed(
                    "two 1 ms periods, the least ALSA allows per buffer \
                     (https://www.alsa-project.org/wiki/FramesPeriods, a search summary \
                     read 2026-10-01, LEAD); replaced by bench session S8",
                ),
            },
            BudgetItem {
                stage: "packetize: one chunk fills",
                ns: self.chunk_ns(),
                source: Source::Computed("chunk_frames / sample_rate_hz"),
            },
            BudgetItem {
                stage: "FEC wait, worst-placed repair",
                ns: self.fec_wait_ns(),
                source: Source::Computed(
                    "(fec_k - 1) x fec_depth x chunk; the block-code wait of RFC 8681 \
                     section 1.2 (https://www.rfc-editor.org/rfc/rfc8681, read 2026-10-01); \
                     one wait end to end, because the relay keeps the groups aligned",
                ),
            },
            BudgetItem {
                stage: "UDP leg, hub to server",
                ns: self.network_leg_ns,
                source: Source::Assumed(
                    "send, two switch hops (12 us each to serialize 1500 B at 1 Gb/s, \
                     computed), receive and the hosts' scheduling; no chorus measurement yet",
                ),
            },
            BudgetItem {
                stage: "server relay: receive, restamp, send",
                ns: self.relay_ns,
                source: Source::Assumed("one wakeup of the relay thread; no measurement yet"),
            },
            BudgetItem {
                stage: "UDP leg, server to endpoint",
                ns: self.network_leg_ns,
                source: Source::Assumed("as the hub's leg"),
            },
            BudgetItem {
                stage: "jitter margin",
                ns: self.jitter_margin_ns,
                source: Source::Assumed(
                    "the research's 2 ms (fec-latency.md section 6), near AES67's suggested \
                     3 ms playout delay (RAVENNA AES67 Practical Guide p. 21, read \
                     2026-10-01); replaced by a LAN loss and delay histogram",
                ),
            },
            BudgetItem {
                stage: "endpoint output buffer and DSP block",
                ns: self.endpoint_output_ns,
                source: Source::Assumed(
                    "BRIEF.md 5.7 (DSP and DAC 2 to 10 ms); a chorus measurement is pending",
                ),
            },
            BudgetItem {
                stage: "DAC digital filter (PCM5102A, normal filter)",
                ns: self.dac_filter_ns,
                source: Source::Cited(
                    "20 tS at 48 kHz, https://www.ti.com/lit/ds/symlink/pcm5102a.pdf \
                     (read 2026-10-01)",
                ),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];

    fn payload(seq: u8, pcm: usize) -> Vec<u8> {
        let mut p = vec![0u8; CHUNK_HEADER_LEN + pcm];
        p[3] = seq;
        for (i, b) in p[CHUNK_HEADER_LEN..].iter_mut().enumerate() {
            *b = seq.wrapping_mul(31).wrapping_add(i as u8);
        }
        p
    }

    fn stream(params: FecParams, chunks: usize) -> (Vec<Vec<u8>>, Vec<Datagram>) {
        let mut enc = FecEncoder::new(params);
        let mut sent = Vec::new();
        let mut out = Vec::new();
        for i in 0..chunks {
            let mut p = payload(i as u8, 8 + (i % 3) * 4);
            out.extend(enc.push_payload(&mut p).unwrap());
            sent.push(p);
        }
        (sent, out)
    }

    #[test]
    fn each_single_loss_of_a_group_is_rebuilt_exactly() {
        let params = FecParams::new(4, 1).unwrap();
        let (sent, dgrams) = stream(params, 8);
        assert_eq!(dgrams.len(), 10);
        for lost in 0..dgrams.len() {
            let mut dec = FecDecoder::new(params);
            let mut got: Vec<Delivered> = Vec::new();
            for (i, d) in dgrams.iter().enumerate() {
                if i != lost {
                    got.extend(dec.push(d.kind, &d.plaintext));
                }
            }
            dec.finish(8);
            got.sort_by_key(|d| d.chunk_index);
            assert_eq!(got.len(), 8, "loss of datagram {}", lost);
            for d in &got {
                assert_eq!(d.payload, sent[d.chunk_index as usize]);
            }
            let s = dec.stats();
            assert_eq!(s.unrecoverable, 0);
            assert_eq!(s.recovered, u64::from(dgrams[lost].kind == Kind::Data));
        }
    }

    #[test]
    fn a_burst_as_long_as_the_depth_is_rebuilt() {
        let params = FecParams::new(3, 2).unwrap();
        let (sent, dgrams) = stream(params, 12);
        // Two consecutive data datagrams: chunks 1 and 2, one per column.
        let data: Vec<usize> = (0..dgrams.len())
            .filter(|&i| dgrams[i].kind == Kind::Data)
            .collect();
        let mut dec = FecDecoder::new(params);
        let mut got = Vec::new();
        for (i, d) in dgrams.iter().enumerate() {
            if i != data[1] && i != data[2] {
                got.extend(dec.push(d.kind, &d.plaintext));
            }
        }
        dec.finish(12);
        assert_eq!(got.len(), 12);
        assert_eq!(dec.stats().recovered, 2);
        for d in &got {
            assert_eq!(d.payload, sent[d.chunk_index as usize]);
        }
    }

    #[test]
    fn two_losses_in_one_group_are_counted_not_guessed() {
        let params = FecParams::new(4, 1).unwrap();
        let (_, dgrams) = stream(params, 8);
        let mut dec = FecDecoder::new(params);
        let mut got = Vec::new();
        for (i, d) in dgrams.iter().enumerate() {
            if i != 0 && i != 1 {
                got.extend(dec.push(d.kind, &d.plaintext));
            }
        }
        dec.finish(8);
        assert_eq!(got.len(), 6);
        assert_eq!(dec.stats().unrecoverable, 2);
        assert_eq!(dec.stats().recovered, 0);
    }

    #[test]
    fn a_whole_lost_group_and_late_packets_are_counted() {
        let params = FecParams::new(2, 1).unwrap();
        let (_, dgrams) = stream(params, 20);
        let mut dec = FecDecoder::new(params);
        // Group 1 (datagrams 3, 4, 5) never arrives until the end.
        for (i, d) in dgrams.iter().enumerate() {
            if !(3..6).contains(&i) {
                dec.push(d.kind, &d.plaintext);
            }
        }
        dec.push(dgrams[3].kind, &dgrams[3].plaintext);
        dec.finish(20);
        assert_eq!(dec.stats().unrecoverable, 2);
        assert_eq!(dec.stats().late, 1);
    }

    #[test]
    fn the_replay_window_takes_reordering_and_refuses_repeats() {
        let mut w = ReplayWindow::new();
        for c in [5u64, 3, 9, 4, 2000, 1000] {
            assert!(w.accepts(c), "{}", c);
            w.commit(c);
        }
        for c in [5u64, 3, 9, 2000, 1000, 976] {
            assert!(!w.accepts(c), "{}", c);
        }
        assert!(w.accepts(977));
        assert!(w.accepts(1999));
        w.commit(1999);
        assert!(!w.accepts(1999));
        w.commit(5000);
        assert!(!w.accepts(1999));
        assert!(w.accepts(4000));
    }

    #[test]
    fn seal_and_open_round_trip_and_refuse_alteration() {
        let header = Header {
            kind: Kind::Data,
            stream_tag: 9,
            counter: 3,
        };
        let sealed = seal(&KEY, &header, b"plaintext").unwrap();
        assert_eq!(sealed.len(), HEADER_LEN + 9 + TAG_LEN);
        assert_eq!(
            open(&KEY, &sealed).unwrap(),
            (header, b"plaintext".to_vec())
        );
        for at in 0..sealed.len() {
            let mut bad = sealed.clone();
            bad[at] ^= 0x01;
            assert!(open(&KEY, &bad).is_err(), "byte {}", at);
        }
        let mut o = Opener::new(KEY, 9);
        assert!(o.open(&sealed).is_ok());
        assert_eq!(o.open(&sealed), Err(OpenError::Replayed));
        let mut other = Opener::new(KEY, 10);
        assert_eq!(other.open(&sealed), Err(OpenError::WrongStreamTag));
    }

    #[test]
    fn the_default_plan_fits_its_own_floor() {
        assert_eq!(DEFAULTS.chunk_ns(), 2_500_000);
        assert_eq!(DEFAULTS.fec_wait_ns(), 7_500_000);
        assert!(DEFAULTS.check_latency(DEFAULTS.l_tv_ns).is_ok());
        assert!(matches!(
            DEFAULTS.check_latency(15_000_000),
            Err(LatencyError::BelowFloor { .. })
        ));
        assert!(DEFAULTS.fec().is_ok());
        assert!(chunk_fits(120, 2, SampleFormat::PcmS24Le));
        assert!(chunk_fits(120, 2, SampleFormat::PcmF32Le));
        assert!(!chunk_fits(120, 6, SampleFormat::PcmS24Le));
    }
}
