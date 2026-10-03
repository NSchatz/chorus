//! The pure half of chorus's Spotify Soloist support.
//!
//! Spotify Soloist is a proprietary Spotify Connect receiver the owner
//! installs; chorus never ships, downloads or links it. A receiver container
//! runs one Soloist process under `chorus-soloistd`, and chorus-server talks
//! to that supervisor through one directory of FIFOs and Unix sockets
//! (`docs/soloist.md`). This crate is everything both sides must agree on,
//! as text in and text out:
//!
//! - [`api`]: the Soloist WebSocket API as documented (commands out, a
//!   tolerant event parser in);
//! - [`ws`]: RFC 6455 framing and the opening handshake as pure functions;
//! - [`protocol`]: the supervisor protocol on `r<i>.sock`;
//! - [`build`]: the `soloist --version` parser and the 90-day expiry
//!   arithmetic;
//! - [`pool`]: which receiver serves which room, saved group or live group;
//! - [`keydir`]: the directory name of a target key;
//! - the receiver directory's file names and the FIFO's sample format, below.
//!
//! Nothing here opens a file, a socket or a clock: a caller hands in bytes,
//! text and instants. That is what lets the server (track S2), the supervisor
//! and the fake Soloist be tested against one set of fixtures
//! (`fixtures/soloist/`, read by `tests/fixtures.rs`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod api;
pub mod build;
pub mod keydir;
pub mod pool;
pub mod protocol;
pub mod ws;

/// Sample rate of the PCM a receiver's FIFO carries, in hertz.
///
/// The pipe-tunnel sink is configured with it (`audio.rate`), so PipeWire
/// converts whatever Soloist plays to this rate. 44.1 kHz is Spotify's
/// stream rate (`docs/soloist.md`, "What is assumed").
pub const PCM_RATE: u32 = 44_100;

/// Channels of the PCM a receiver's FIFO carries, interleaved left then right.
pub const PCM_CHANNELS: usize = 2;

/// Bytes of one sample in the FIFO: an IEEE 754 binary32, little-endian.
pub const PCM_SAMPLE_BYTES: usize = 4;

/// Bytes of one frame (one sample per channel) in the FIFO.
pub const PCM_FRAME_BYTES: usize = PCM_SAMPLE_BYTES * PCM_CHANNELS;

/// The FIFO's sample format as PipeWire spells it (`audio.format`).
pub const PIPEWIRE_FORMAT: &str = "F32LE";

/// The file in Soloist's data directory holding the WebSocket port it bound.
pub const WS_PORT_FILE: &str = "ws.port";

/// The file in Soloist's data directory holding the WebSocket bind address.
pub const WS_ADDR_FILE: &str = "ws.addr";

/// The file in Soloist's data directory naming the process that uses it.
pub const PID_FILE: &str = "soloist.pid";

/// A receiver's id as logs, flags and the state spell it: `r<index>`.
pub fn receiver_id(index: usize) -> String {
    format!("r{index}")
}

/// The index of a receiver id (`r0`, `r1`, ...), if it is one.
///
/// Only the canonical spelling is an id: no sign, no leading zero.
pub fn receiver_index(id: &str) -> Option<usize> {
    let digits = id.strip_prefix('r')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    digits.parse().ok()
}

/// The lock file a supervisor holds for its lifetime: `r<index>.lock`.
pub fn lock_file_name(index: usize) -> String {
    format!("r{index}.lock")
}

/// The PCM FIFO of a receiver: `r<index>.pcm`.
pub fn pcm_file_name(index: usize) -> String {
    format!("r{index}.pcm")
}

/// The supervisor's Unix socket of a receiver: `r<index>.sock`.
pub fn socket_file_name(index: usize) -> String {
    format!("r{index}.sock")
}

/// The PipeWire node name of a receiver's pipe-tunnel sink, which Soloist is
/// started with (`--pipewire-device`): `chorus-r<index>`.
pub fn node_name(index: usize) -> String {
    format!("chorus-r{index}")
}

/// One interleaved little-endian float32 frame as the FIFO carries it.
pub fn encode_frame(left: f32, right: f32) -> [u8; PCM_FRAME_BYTES] {
    let mut out = [0u8; PCM_FRAME_BYTES];
    out[..4].copy_from_slice(&left.to_le_bytes());
    out[4..].copy_from_slice(&right.to_le_bytes());
    out
}

/// The two samples of one FIFO frame.
pub fn decode_frame(frame: [u8; PCM_FRAME_BYTES]) -> (f32, f32) {
    (
        f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]),
        f32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_of_a_receiver() {
        assert_eq!(receiver_id(0), "r0");
        assert_eq!(lock_file_name(3), "r3.lock");
        assert_eq!(pcm_file_name(3), "r3.pcm");
        assert_eq!(socket_file_name(12), "r12.sock");
        assert_eq!(node_name(12), "chorus-r12");
    }

    #[test]
    fn a_receiver_id_reads_back_only_in_its_one_spelling() {
        for i in [0usize, 1, 9, 10, 15, 255] {
            assert_eq!(receiver_index(&receiver_id(i)), Some(i));
        }
        for bad in [
            "", "r", "r01", "r-1", "r+1", "R1", "1", "r1 ", "rr1", "r1.5",
        ] {
            assert_eq!(receiver_index(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_frame_is_two_little_endian_floats() {
        let bytes = encode_frame(1.0, -0.5);
        assert_eq!(bytes, [0, 0, 0x80, 0x3f, 0, 0, 0, 0xbf]);
        assert_eq!(decode_frame(bytes), (1.0, -0.5));
        assert_eq!(PCM_FRAME_BYTES, 8);
    }
}
