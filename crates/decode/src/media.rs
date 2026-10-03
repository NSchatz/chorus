//! What a decoder reads: bytes, and maybe a way to move among them.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Mutex;

use symphonia::core::io::MediaSource;

/// The bytes of one piece of media. A file and an HTTP body with range
/// requests are seekable; a radio stream is not, and has no length.
pub trait Media: Read + Send {
    /// Move to `pos`. An error when the media is not seekable.
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
    /// Whether [`Media::seek`] works.
    fn is_seekable(&self) -> bool;
    /// The length in bytes, when it is known.
    fn byte_len(&self) -> Option<u64>;
}

impl Media for std::fs::File {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(self, pos)
    }
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        self.metadata().ok().map(|m| m.len())
    }
}

impl Media for io::Cursor<Vec<u8>> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(self, pos)
    }
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.get_ref().len() as u64)
    }
}

/// Reads until `buf` is full or the media ends; returns how much was read.
pub(crate) fn read_full(media: &mut dyn Media, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match media.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// A [`Media`] as Symphonia's byte source. The decoder looks at the first
/// bytes before Symphonia does (to name what it refuses); on media that cannot
/// seek back, those bytes are replayed from `head`.
///
/// Symphonia asks a source to be `Sync`; a `Media` is only `Send`. The mutex
/// supplies that without a lock ever being contended or even taken: every use
/// goes through `get_mut` on an exclusive borrow.
pub(crate) struct Source {
    media: Mutex<Box<dyn Media>>,
    head: Vec<u8>,
    head_pos: usize,
    seekable: bool,
    byte_len: Option<u64>,
}

impl Source {
    /// `head` is what was already read from a media that cannot seek; empty
    /// for a seekable one (which was moved back to its start instead).
    pub(crate) fn new(media: Box<dyn Media>, head: Vec<u8>) -> Source {
        let seekable = media.is_seekable();
        let byte_len = media.byte_len();
        Source {
            media: Mutex::new(media),
            head,
            head_pos: 0,
            seekable,
            byte_len,
        }
    }

    fn media(&mut self) -> &mut Box<dyn Media> {
        self.media.get_mut().unwrap_or_else(|e| e.into_inner())
    }
}

impl Read for Source {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.head_pos < self.head.len() {
            let n = buf.len().min(self.head.len() - self.head_pos);
            buf[..n].copy_from_slice(&self.head[self.head_pos..self.head_pos + n]);
            self.head_pos += n;
            return Ok(n);
        }
        self.media().read(buf)
    }
}

impl Seek for Source {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        if !self.seekable {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the media is not seekable",
            ));
        }
        self.media().seek(pos)
    }
}

impl MediaSource for Source {
    fn is_seekable(&self) -> bool {
        self.seekable
    }
    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}
