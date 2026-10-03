//! Base64 (RFC 4648 section 4, the standard alphabet with `=` padding).
//!
//! The OpenHome Playlist's `IdArray` is a `bin.base64` value: the track ids
//! as big-endian 32-bit numbers, concatenated, in base64 (ohPipeline
//! `OpenHome/Av/Playlist/ProviderPlaylist.cpp:526-536` at `cccd06dd`). The
//! workspace has no base64 crate and this is thirty lines, so it is written
//! here from the RFC and held to the RFC's own test vectors (section 10).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` in base64, padded (RFC 4648 section 4).
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let sextet = |shift: u32| ALPHABET[((n >> shift) & 0x3f) as usize] as char;
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

fn value(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// The bytes a base64 text stands for, or `None` when it is not base64.
/// White space between characters is skipped (a value inside XML is often
/// wrapped); padding is required where the length calls for it, nothing may
/// follow it, and the bits a final group leaves over must be zero (RFC 4648
/// section 3.5), so one text has one reading.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let symbols: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !symbols.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(symbols.len() / 4 * 3);
    let groups = symbols.len() / 4;
    for (i, group) in symbols.chunks(4).enumerate() {
        let padding = group.iter().rev().take_while(|b| **b == b'=').count();
        if padding > 2 || (padding > 0 && i + 1 != groups) {
            return None;
        }
        let mut n = 0u32;
        for b in &group[..4 - padding] {
            n = (n << 6) | value(*b)?;
        }
        n <<= 6 * padding as u32;
        match padding {
            0 => out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]),
            1 => {
                if n & 0xff != 0 {
                    return None;
                }
                out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8]);
            }
            _ => {
                if n & 0xffff != 0 {
                    return None;
                }
                out.push((n >> 16) as u8);
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648 section 10, "Test Vectors", the BASE64 rows, all seven.
    const RFC_4648_SECTION_10: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];

    #[test]
    fn the_rfc_4648_vectors_encode_and_decode() {
        for (plain, encoded) in RFC_4648_SECTION_10 {
            assert_eq!(encode(plain.as_bytes()), encoded, "{plain}");
            assert_eq!(
                decode(encoded).as_deref(),
                Some(plain.as_bytes()),
                "{plain}"
            );
        }
    }

    #[test]
    fn every_byte_value_survives_a_round_trip() {
        let all: Vec<u8> = (0..=255u8).collect();
        for len in [0usize, 1, 2, 3, 4, 5, 254, 255, 256] {
            let bytes = &all[..len];
            assert_eq!(decode(&encode(bytes)).as_deref(), Some(bytes), "{len}");
        }
        // The two symbols past the letters and digits.
        assert_eq!(encode(&[0xfb, 0xff, 0xfe]), "+//+");
        assert_eq!(decode("+//+"), Some(vec![0xfb, 0xff, 0xfe]));
    }

    #[test]
    fn what_is_not_base64_is_refused() {
        for bad in [
            "Z", "Zg", "Zg=", "Zm8", "====", "Zg==Zg==", "Z=g=", "Zm9v=", "Zm-v", "Zm9v!", "=Zg=",
            // Left-over bits that are not zero: two texts for one value.
            "Zh==", "Zm9=",
        ] {
            assert_eq!(decode(bad), None, "{bad}");
        }
        // White space is skipped.
        assert_eq!(decode(" Zm9v\r\nYmFy\t").as_deref(), Some(&b"foobar"[..]));
    }
}
