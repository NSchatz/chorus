//! ICY (SHOUTcast and Icecast) in-band metadata: the blocks a server
//! interleaves with the audio when the request said `Icy-MetaData: 1`, taken
//! back out so the decoder sees audio only.
//!
//! There is no standard. Worked from the write-up at
//! https://cast.readme.io/docs/icy (read 2026-10-03): the response header
//! `icy-metaint: N` gives the number of audio bytes between metadata blocks,
//! and `icy-name`, `icy-genre`, `icy-br` describe the station.
//!
//! ASSUMED (from that page's examples and common knowledge of the format, no
//! normative text exists): after every N audio bytes comes one length byte L,
//! then `L * 16` bytes of text padded with NUL, holding
//! `StreamTitle='...';` and possibly `StreamUrl='...';`; L = 0 means nothing
//! changed. The text's encoding is not declared: it is read as UTF-8 and, when
//! it is not valid UTF-8, as Latin-1. A title may itself contain `';`, so the
//! title ends at `';StreamUrl='` when that follows and at the last `';`
//! otherwise.

/// The station's description from the response headers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IcyInfo {
    /// `icy-name`.
    pub name: Option<String>,
    /// `icy-br`, in kbit/s.
    pub bitrate_kbps: Option<u32>,
    /// `icy-genre`.
    pub genre: Option<String>,
    /// `icy-metaint`: audio bytes between metadata blocks, when the server
    /// interleaves them.
    pub metaint: Option<usize>,
}

/// The largest `icy-metaint` accepted. ASSUMED: servers use 8192 or 16000;
/// a megabyte is far above both and bounds nothing in memory (the filter keeps
/// only a metadata block, 4080 bytes at most).
pub const MAX_METAINT: usize = 1 << 20;

enum State {
    /// This many audio bytes remain before the next length byte.
    Audio(usize),
    /// The next byte is a block's length.
    Length,
    /// This many bytes of the block remain.
    Block(usize),
}

/// Removes the metadata blocks from a stream, whatever the read sizes.
pub struct IcyFilter {
    metaint: usize,
    state: State,
    block: Vec<u8>,
    title: Option<String>,
}

impl IcyFilter {
    /// A filter for a stream with `metaint` audio bytes between blocks
    /// (`metaint` at least 1).
    pub fn new(metaint: usize) -> IcyFilter {
        IcyFilter {
            metaint,
            state: State::Audio(metaint),
            block: Vec::new(),
            title: None,
        }
    }

    /// The latest `StreamTitle`, once a block carried one.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Takes the next `len` bytes of the stream in `buf[..len]` and leaves
    /// only the audio among them at the front of `buf`; returns how many
    /// bytes that is.
    pub fn strip(&mut self, buf: &mut [u8], len: usize) -> usize {
        let mut read = 0;
        let mut kept = 0;
        while read < len {
            match self.state {
                State::Audio(left) => {
                    let take = left.min(len - read);
                    buf.copy_within(read..read + take, kept);
                    read += take;
                    kept += take;
                    self.state = if left == take {
                        State::Length
                    } else {
                        State::Audio(left - take)
                    };
                }
                State::Length => {
                    let block = usize::from(buf[read]) * 16;
                    read += 1;
                    self.block.clear();
                    self.state = if block == 0 {
                        State::Audio(self.metaint)
                    } else {
                        State::Block(block)
                    };
                }
                State::Block(left) => {
                    let take = left.min(len - read);
                    self.block.extend_from_slice(&buf[read..read + take]);
                    read += take;
                    if left == take {
                        if let Some(title) = stream_title(&self.block) {
                            self.title = Some(title);
                        }
                        self.block.clear();
                        self.state = State::Audio(self.metaint);
                    } else {
                        self.state = State::Block(left - take);
                    }
                }
            }
        }
        kept
    }
}

/// The `StreamTitle` of one metadata block, when it has one.
pub fn stream_title(block: &[u8]) -> Option<String> {
    let end = block.iter().position(|&b| b == 0).unwrap_or(block.len());
    let text = match std::str::from_utf8(&block[..end]) {
        Ok(text) => text.to_string(),
        // Latin-1: every byte is the code point of the same number.
        Err(_) => block[..end].iter().map(|&b| b as char).collect(),
    };
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    let title = match rest.find("';StreamUrl='") {
        Some(end) => &rest[..end],
        None => match rest.rfind("';") {
            Some(end) => &rest[..end],
            None => rest.strip_suffix('\'').unwrap_or(rest),
        },
    };
    Some(title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A metadata block as a server writes it: the length byte and the padded text.
    pub(crate) fn block(text: &str) -> Vec<u8> {
        let units = text.len().div_ceil(16);
        let mut out = vec![units as u8];
        out.extend_from_slice(text.as_bytes());
        out.resize(1 + units * 16, 0);
        out
    }

    /// `audio` with a block after every `metaint` bytes, taken from `blocks`
    /// in turn (an empty text is the zero-length block).
    fn interleave(audio: &[u8], metaint: usize, blocks: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, chunk) in audio.chunks(metaint).enumerate() {
            out.extend_from_slice(chunk);
            if chunk.len() == metaint {
                out.extend_from_slice(&block(blocks[i % blocks.len()]));
            }
        }
        out
    }

    fn audio(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + 3) as u8).collect()
    }

    #[test]
    fn the_audio_comes_back_whatever_the_read_size() {
        let blocks = [
            "StreamTitle='One';",
            "",
            "StreamTitle='Two - Three';StreamUrl='';",
        ];
        for metaint in [1, 5, 16, 100, 8192] {
            let want = audio(metaint * 7 + metaint / 2);
            let wire = interleave(&want, metaint, &blocks);
            for read_size in [1, 2, 3, 7, 16, 17, 100, 4096, wire.len()] {
                let mut filter = IcyFilter::new(metaint);
                let mut got = Vec::new();
                for piece in wire.chunks(read_size) {
                    let mut buf = piece.to_vec();
                    let kept = filter.strip(&mut buf, piece.len());
                    got.extend_from_slice(&buf[..kept]);
                }
                assert_eq!(got, want, "metaint {metaint}, reads of {read_size}");
                assert_eq!(filter.title(), Some("One"), "the last non-empty block of 7");
            }
        }
    }

    #[test]
    fn a_zero_length_block_keeps_the_title() {
        let mut filter = IcyFilter::new(4);
        let mut wire = Vec::new();
        wire.extend_from_slice(b"aaaa");
        wire.extend_from_slice(&block("StreamTitle='Kept';"));
        wire.extend_from_slice(b"bbbb");
        wire.push(0);
        wire.extend_from_slice(b"cc");
        let len = wire.len();
        let kept = filter.strip(&mut wire, len);
        assert_eq!(&wire[..kept], b"aaaabbbbcc");
        assert_eq!(filter.title(), Some("Kept"));
    }

    #[test]
    fn the_title_updates_block_by_block() {
        let mut filter = IcyFilter::new(2);
        assert_eq!(filter.title(), None);
        for (text, want) in [("StreamTitle='A';", "A"), ("StreamTitle='B';", "B")] {
            let mut wire = b"xy".to_vec();
            wire.extend_from_slice(&block(text));
            let len = wire.len();
            assert_eq!(filter.strip(&mut wire, len), 2);
            assert_eq!(filter.title(), Some(want));
        }
    }

    #[test]
    fn titles_parse() {
        let t = |text: &[u8]| stream_title(text);
        assert_eq!(
            t(b"StreamTitle='Artist - Song';\0\0"),
            Some("Artist - Song".into())
        );
        assert_eq!(
            t(b"StreamTitle='It's here'; now';StreamUrl='http://radio.example/';"),
            Some("It's here'; now".into())
        );
        assert_eq!(t(b"StreamTitle='a';b';"), Some("a';b".into()));
        assert_eq!(t(b"StreamTitle='';"), Some(String::new()));
        assert_eq!(t(b"StreamTitle='unterminated"), Some("unterminated".into()));
        assert_eq!(t(b"StreamUrl='http://radio.example/';"), None);
        assert_eq!(t(b""), None);
        // Latin-1 e-acute is not valid UTF-8 on its own.
        assert_eq!(t(b"StreamTitle='Caf\xe9';"), Some("Caf\u{e9}".into()));
        assert_eq!(
            t("StreamTitle='Caf\u{e9}';".as_bytes()),
            Some("Caf\u{e9}".into())
        );
    }
}
