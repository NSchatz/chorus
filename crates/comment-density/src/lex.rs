// The Rust scanner every count rests on. It answers one question per byte
// range: comment, or code? Whatever is neither is whitespace and belongs to no
// line.
//
// It is a lexer and not a parser: a file that lexes but does not compile is
// still counted, because a comment is a lexical thing and a gate that needed
// the tree to build would go quiet exactly when the tree is broken.
//
// Matching a line against a pattern is what this exists to avoid. The closing
// quote of a multi-line string reads exactly like a comment opening one, and
// guessing scored mostly-code files at 85% prose.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Comment,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lexeme {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub offset: usize,
    pub what: &'static str,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "byte {}: {}", self.offset, self.what)
    }
}

pub fn scan(src: &str) -> Result<Vec<Lexeme>, LexError> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if b[i] == b'/' && i + 1 < n && b[i + 1] == b'/' {
            let mut j = i + 2;
            while j < n && b[j] != b'\n' {
                j += 1;
            }
            out.push(Lexeme { kind: Kind::Comment, start, end: j });
            i = j;
            continue;
        }
        if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let end = block_comment_end(b, i)?;
            out.push(Lexeme { kind: Kind::Comment, start, end });
            i = end;
            continue;
        }
        let end = match b[i] {
            b'"' => string_end(b, i)?,
            b'\'' => quote_end(b, i)?,
            b'r' | b'b' | b'c' => prefixed_end(b, i)?,
            _ => plain_end(b, i),
        };
        out.push(Lexeme { kind: Kind::Code, start, end });
        i = end;
    }
    Ok(out)
}

// Rust permits nesting, so a depth counter is the whole of it. A string inside
// a block comment is not a string, which is why nothing here looks for one.
fn block_comment_end(b: &[u8], start: usize) -> Result<usize, LexError> {
    let n = b.len();
    let mut depth = 1usize;
    let mut j = start + 2;
    while j < n {
        if b[j] == b'/' && j + 1 < n && b[j + 1] == b'*' {
            depth += 1;
            j += 2;
        } else if b[j] == b'*' && j + 1 < n && b[j + 1] == b'/' {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return Ok(j);
            }
        } else {
            j += 1;
        }
    }
    Err(LexError { offset: start, what: "unterminated block comment" })
}

fn string_end(b: &[u8], quote: usize) -> Result<usize, LexError> {
    let n = b.len();
    let mut j = quote + 1;
    while j < n {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return Ok(j + 1),
            _ => j += 1,
        }
    }
    Err(LexError { offset: quote, what: "unterminated string literal" })
}

// A raw string ends at a quote followed by exactly the hash count it opened
// with, and nothing inside it escapes. `r#"// not a comment"#` is the case the
// umbrella's pattern matcher got wrong.
fn raw_string_end(b: &[u8], start: usize, quote: usize, hashes: usize) -> Result<usize, LexError> {
    let n = b.len();
    let mut j = quote + 1;
    while j < n {
        if b[j] == b'"' {
            let mut k = j + 1;
            let mut seen = 0;
            while k < n && seen < hashes && b[k] == b'#' {
                seen += 1;
                k += 1;
            }
            if seen == hashes {
                return Ok(k);
            }
        }
        j += 1;
    }
    Err(LexError { offset: start, what: "unterminated raw string literal" })
}

// `'a` is a lifetime and `'a'` is a character. The difference is the byte after
// the character, and reading it wrong swallows the rest of the file.
fn quote_end(b: &[u8], start: usize) -> Result<usize, LexError> {
    let n = b.len();
    if start + 1 >= n {
        return Ok(start + 1);
    }
    if b[start + 1] == b'\\' {
        return escaped_char_end(b, start);
    }
    let after = start + 1 + char_len(b[start + 1]);
    if after < n && b[after] == b'\'' {
        return Ok(after + 1);
    }
    let mut j = start + 1;
    while j < n && is_ident(b[j]) {
        j += 1;
    }
    Ok(j)
}

fn escaped_char_end(b: &[u8], start: usize) -> Result<usize, LexError> {
    let n = b.len();
    let mut j = start + 1;
    while j < n {
        match b[j] {
            b'\\' => j += 2,
            b'\'' => return Ok(j + 1),
            _ => j += 1,
        }
    }
    Err(LexError { offset: start, what: "unterminated character literal" })
}

// The literal prefixes, and the identifiers that begin with the same letters.
// `crate`, `break` and `r` itself all arrive here, and each has to leave as an
// identifier.
fn prefixed_end(b: &[u8], start: usize) -> Result<usize, LexError> {
    let n = b.len();
    let mut j = start;
    if (b[j] == b'b' || b[j] == b'c') && j + 1 < n {
        match b[j + 1] {
            b'"' => return string_end(b, j + 1),
            b'\'' if b[j] == b'b' => return quote_end(b, j + 1),
            b'r' => j += 1,
            _ => return Ok(plain_end(b, start)),
        }
    }
    if b[j] == b'r' {
        let mut k = j + 1;
        let mut hashes = 0;
        while k < n && b[k] == b'#' {
            hashes += 1;
            k += 1;
        }
        if k < n && b[k] == b'"' {
            return raw_string_end(b, start, k, hashes);
        }
    }
    Ok(plain_end(b, start))
}

fn plain_end(b: &[u8], start: usize) -> usize {
    if !is_ident(b[start]) {
        return start + 1;
    }
    let mut j = start;
    while j < b.len() && is_ident(b[j]) {
        j += 1;
    }
    j
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

fn char_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}
