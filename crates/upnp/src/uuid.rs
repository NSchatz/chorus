//! SHA-1, UUID version 5, and the identity of a chorus renderer.
//!
//! A control point remembers a renderer by its UDN, and UDA11 section 1.1.4
//! says a device's UUID "MUST remain fixed over time". chorus's renderers are
//! not hardware: a room, a saved group, or a live group that exists only
//! while its members play together. So the UDN is a name-based UUID
//! (RFC9562 section 5.5) over what the target *is*: the same room always gets
//! the same UDN, with no state to keep, and a live group that re-forms from
//! the same rooms is the same device again.
//!
//! SHA-1 is written here because version 5 is defined over it; it guards
//! nothing (RFC9562 section 5.5 itself notes SHA-1 is not to be relied on for
//! security), it only spreads names over the UUID space.

use std::fmt;

/// SHA-1 of `data`, per FIPS 180-4 section 6.1 as restated in RFC 3174
/// (sections 4 to 6: the padding, the functions and constants, method 1).
/// Tested against RFC 3174 section 7.3's vectors and the SHA-1 line of
/// RFC9562 appendix A.4.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    // RFC 3174 section 6.1: the initial hash value.
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    // RFC 3174 section 4: a 1 bit, zeros to 56 mod 64 bytes, then the length
    // in bits as 64 bits, most significant first.
    let bits = (data.len() as u64).wrapping_mul(8);
    let mut tail = [0u8; 128];
    let rest = data.len() % 64;
    let whole = data.len() - rest;
    tail[..rest].copy_from_slice(&data[whole..]);
    tail[rest] = 0x80;
    let tail_len = if rest < 56 { 64 } else { 128 };
    tail[tail_len - 8..tail_len].copy_from_slice(&bits.to_be_bytes());

    let blocks = data[..whole]
        .chunks_exact(64)
        .chain(tail[..tail_len].chunks_exact(64));
    for block in blocks {
        let mut w = [0u32; 80];
        for (t, word) in block.chunks_exact(4).enumerate() {
            w[t] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for t in 16..80 {
            w[t] = (w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (t, wt) in w.iter().enumerate() {
            // RFC 3174 section 5: f(t) and K(t) by round.
            let (f, k) = match t {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(*wt)
                .wrapping_add(k);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (hi, v) in h.iter_mut().zip([a, b, c, d, e]) {
            *hi = hi.wrapping_add(v);
        }
    }
    let mut out = [0u8; 20];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// A UUID as its 16 octets in network order (RFC9562 section 4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uuid(pub [u8; 16]);

impl Uuid {
    /// Parses the hyphenated 8-4-4-4-12 hex form, either case (UDA11 section
    /// 1.1.4 fixes that form for UPnP).
    pub fn parse(text: &str) -> Option<Uuid> {
        let b = text.as_bytes();
        if b.len() != 36 {
            return None;
        }
        let mut out = [0u8; 16];
        let mut n = 0;
        let mut i = 0;
        while i < 36 {
            if matches!(i, 8 | 13 | 18 | 23) {
                if b[i] != b'-' {
                    return None;
                }
                i += 1;
                continue;
            }
            let hi = (b[i] as char).to_digit(16)?;
            let lo = (b[i + 1] as char).to_digit(16)?;
            out[n] = (hi * 16 + lo) as u8;
            n += 1;
            i += 2;
        }
        Some(Uuid(out))
    }

    /// A version 4 UUID from 16 random octets the caller supplies (RFC9562
    /// section 5.4: all bits random but the version and the variant). The
    /// subscription identifiers of eventing are these.
    pub fn from_random(mut bytes: [u8; 16]) -> Uuid {
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        Uuid(bytes)
    }
}

impl fmt::Display for Uuid {
    /// Lower-case hex, 8-4-4-4-12.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Uuid({self})")
    }
}

/// UUID version 5 (RFC9562 section 5.5): SHA-1 over the namespace's 16 octets
/// followed by the name; the leftmost 128 bits of the hash; then the version
/// (section 4.2: the high four bits of octet 6 become 0101) and the variant
/// (section 4.1: the high two bits of octet 8 become 10).
pub fn uuid_v5(namespace: &Uuid, name: &[u8]) -> Uuid {
    let mut input = Vec::with_capacity(16 + name.len());
    input.extend_from_slice(&namespace.0);
    input.extend_from_slice(name);
    let hash = sha1(&input);
    let mut u = [0u8; 16];
    u.copy_from_slice(&hash[..16]);
    u[6] = (u[6] & 0x0F) | 0x50;
    u[8] = (u[8] & 0x3F) | 0x80;
    Uuid(u)
}

/// The namespace every chorus renderer's UDN is made in.
///
/// How it was made: one version 4 UUID drawn from the operating system's
/// random source on 2026-10-03 (Python's `uuid.uuid4()`), written here once.
/// It carries no meaning and it must never change: changing it changes every
/// renderer's UDN, and every control point would see a household of new
/// devices. Two chorus servers on one network stay apart through the server
/// id in the name, not through the namespace.
pub const CHORUS_NAMESPACE: Uuid = Uuid([
    0x77, 0x37, 0x0a, 0x15, 0xdd, 0xff, 0x42, 0x9d, 0xb8, 0xdd, 0x61, 0x9e, 0xe6, 0x83, 0x0a, 0x62,
]);

/// What a renderer stands for: the three kinds of target the control plane
/// has (K59: every room, saved group and live group is a target).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target<'a> {
    /// A room, by its stable id (never its display name).
    Room(&'a str),
    /// A saved group, by its stable id.
    Group(&'a str),
    /// A live group, by its member rooms' ids in any order.
    Live(&'a [&'a str]),
}

impl Target<'_> {
    /// The target's name: `room:<id>`, `group:<id>`, or
    /// `live:<member ids sorted by byte order, each once, joined by +>`. The
    /// sort is done here, so two orderings of one member set are one name
    /// and so one UDN.
    pub fn name(&self) -> String {
        match self {
            Target::Room(id) => format!("room:{id}"),
            Target::Group(id) => format!("group:{id}"),
            Target::Live(members) => {
                let mut ids: Vec<&str> = members.to_vec();
                ids.sort_unstable();
                ids.dedup();
                format!("live:{}", ids.join("+"))
            }
        }
    }
}

/// The UDN's UUID for a target of one server: UUID version 5 in `namespace`
/// over the UTF-8 bytes of `<server_id>/<target name>`. The server id keeps
/// two chorus servers with a room of the same id apart; it is whatever stable
/// identity the server has (it is not secret and is not recoverable from the
/// UUID). A live group whose members equal a saved group's is still a
/// different device: the prefix differs.
pub fn udn(namespace: &Uuid, server_id: &str, target: &Target<'_>) -> Uuid {
    uuid_v5(
        namespace,
        format!("{server_id}/{}", target.name()).as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// RFC 3174 section 7.3, TEST1 to TEST4 (the same messages are FIPS
    /// 180's examples): "abc", the 56-octet message, one million "a", and
    /// 640 octets of "01234567" repeated. The empty message's digest is the
    /// well-known one, cross-checked with Python's hashlib on 2026-10-03.
    #[test]
    fn sha1_matches_the_published_vectors() {
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        assert_eq!(
            hex(&sha1(&vec![b'a'; 1_000_000])),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
        assert_eq!(
            hex(&sha1(
                &b"0123456701234567012345670123456701234567012345670123456701234567".repeat(10)
            )),
            "dea356a2cddd90c7a7ecedc5ebb563934f460452"
        );
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    /// Every padding case: lengths around the 55/56 and 63/64 octet edges
    /// hash without panicking and differ from one another.
    #[test]
    fn sha1_pads_at_every_block_edge() {
        let mut seen = std::collections::BTreeSet::new();
        for len in 0..200 {
            assert!(seen.insert(sha1(&vec![0x5a; len])));
        }
    }

    /// RFC9562 appendix A.4: namespace DNS, name "www.example.com".
    #[test]
    fn uuid_v5_matches_the_rfc_9562_vector() {
        let dns = Uuid::parse("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let mut input = dns.0.to_vec();
        input.extend_from_slice(b"www.example.com");
        assert_eq!(
            hex(&sha1(&input)),
            "2ed6657de927468b55e12665a8aea6a22dee3e35"
        );
        assert_eq!(
            uuid_v5(&dns, b"www.example.com").to_string(),
            "2ed6657d-e927-568b-95e1-2665a8aea6a2"
        );
    }

    #[test]
    fn a_uuid_parses_and_prints_in_the_hyphenated_form() {
        let text = "77370a15-ddff-429d-b8dd-619ee6830a62";
        assert_eq!(Uuid::parse(text), Some(CHORUS_NAMESPACE));
        assert_eq!(CHORUS_NAMESPACE.to_string(), text);
        assert_eq!(
            Uuid::parse("77370A15-DDFF-429D-B8DD-619EE6830A62"),
            Some(CHORUS_NAMESPACE)
        );
        for bad in [
            "",
            "77370a15",
            "77370a15-ddff-429d-b8dd-619ee6830a6g",
            "77370a15xddff-429d-b8dd-619ee6830a62",
        ] {
            assert_eq!(Uuid::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_random_uuid_is_version_4_variant_10() {
        let u = Uuid::from_random([0xff; 16]);
        assert_eq!(u.to_string(), "ffffffff-ffff-4fff-bfff-ffffffffffff");
        let u = Uuid::from_random([0; 16]);
        assert_eq!(u.to_string(), "00000000-0000-4000-8000-000000000000");
    }

    /// The UDNs below were computed with Python's `uuid.uuid5` over the same
    /// namespace and names on 2026-10-03, not with this crate.
    #[test]
    fn udns_are_stable_and_named_by_kind() {
        let ns = &CHORUS_NAMESPACE;
        assert_eq!(
            udn(ns, "srv1", &Target::Room("kitchen")).to_string(),
            "3b8fa6e6-bb30-5005-b768-3e87f0af9a9a"
        );
        assert_eq!(
            udn(ns, "srv1", &Target::Group("downstairs")).to_string(),
            "f249f579-88bf-599b-ab64-329f36c0c79e"
        );
        assert_eq!(
            udn(ns, "srv1", &Target::Live(&["kitchen", "bath"])).to_string(),
            "96c438d4-de5d-50f7-a594-aca975addde4"
        );
        assert_eq!(
            udn(ns, "", &Target::Room("kitchen")).to_string(),
            "f416c433-31fd-5289-84bc-4db340175230"
        );
    }

    #[test]
    fn two_orderings_of_one_member_set_are_one_udn() {
        let ns = &CHORUS_NAMESPACE;
        let a = udn(ns, "s", &Target::Live(&["kitchen", "bath", "hall"]));
        let b = udn(ns, "s", &Target::Live(&["hall", "kitchen", "bath"]));
        let c = udn(ns, "s", &Target::Live(&["bath", "hall", "kitchen", "bath"]));
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(
            Target::Live(&["kitchen", "bath", "hall"]).name(),
            "live:bath+hall+kitchen"
        );
        // A different set, a different server, or a different kind differs.
        assert_ne!(a, udn(ns, "s", &Target::Live(&["kitchen", "bath"])));
        assert_ne!(a, udn(ns, "t", &Target::Live(&["kitchen", "bath", "hall"])));
        assert_ne!(
            udn(ns, "s", &Target::Room("x")),
            udn(ns, "s", &Target::Group("x"))
        );
        assert_ne!(
            udn(ns, "s", &Target::Live(&["x"])),
            udn(ns, "s", &Target::Room("x"))
        );
    }
}
