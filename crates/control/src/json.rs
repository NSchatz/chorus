//! A JSON reader and a canonical JSON writer, written here rather than
//! vendored.
//!
//! # Why this is in the repository
//!
//! Two reasons, and neither is that a crate would have been hard to add.
//!
//! The first is `docs/decisions/0002-repository-layout-and-ci.md`: this
//! workspace has no third-party dependency at all, and every `[dependencies]`
//! entry in it is a path to another crate in the same tree.
//!
//! The second is the one that would apply even if it did. `fixtures/control/`
//! pins the control catalog the way `fixtures/protocol/` pins the audio wire:
//! byte for byte, so a second implementation in another language is graded
//! against the bytes rather than against this code. A golden vector needs an
//! encoder whose output is DECIDED - key order, number spelling, which
//! characters are escaped - and a serialiser that decides those from a derived
//! struct's field order decides them somewhere else. `docs/control-plane.md`
//! states each of those rules and [`write`] implements exactly them.
//!
//! # What the reader accepts
//!
//! RFC 8259 JSON text, strictly, with three deliberate additions to the "no"
//! list:
//!
//! - a duplicate key is refused rather than resolved. Two spellings of one
//!   field have two meanings and no way to choose, and a control message is
//!   not a place to guess;
//! - nesting deeper than [`MAX_DEPTH`] is refused, because a recursive-descent
//!   reader handed a few thousand brackets from a socket is a stack overflow
//!   and not an error;
//! - trailing content after the value is refused, so two messages
//!   concatenated cannot read as one.
//!
//! What it does not do is convert numbers. A number keeps the exact digits it
//! arrived as, and [`Value::Num`] carries them, so a value that goes in and
//! comes out again is the same text. That is what lets a volume be a decimal
//! with a declared number of places rather than a float whose printed form
//! depends on the printer.

use std::fmt;

/// Deepest nesting the reader will follow.
///
/// A control message in this catalog nests three levels at most (the message
/// object, its `zones` array, a zone object), so this is a very long way above
/// anything legitimate. It exists for the illegitimate case.
pub const MAX_DEPTH: usize = 32;

/// A JSON value.
///
/// The object variant keeps its pairs in order, because canonical output has a
/// declared key order and a map would lose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number, as the digits it was written with.
    Num(String),
    /// A string, unescaped.
    Str(String),
    /// An array.
    Arr(Vec<Value>),
    /// An object, in the order its members were written.
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// A number value from a whole number.
    pub fn int(v: i64) -> Value {
        Value::Num(v.to_string())
    }

    /// A string value.
    pub fn text(v: &str) -> Value {
        Value::Str(v.to_string())
    }

    /// The member of an object with this key, if this is an object with one.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// This value as a string, if it is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// This value as a boolean, if it is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// This value's digits, if it is a number.
    pub fn as_num(&self) -> Option<&str> {
        match self {
            Value::Num(n) => Some(n),
            _ => None,
        }
    }

    /// The name of this value's kind, for an error that has to say what it
    /// found.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "a boolean",
            Value::Num(_) => "a number",
            Value::Str(_) => "a string",
            Value::Arr(_) => "an array",
            Value::Obj(_) => "an object",
        }
    }
}

/// Why a text is not JSON this reader accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    /// Byte offset the reader stopped at.
    pub at: usize,
    /// What was wrong, in words.
    pub detail: String,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.detail, self.at)
    }
}

impl std::error::Error for JsonError {}

/// Read one JSON value, and require that it is the whole text.
pub fn parse(text: &str) -> Result<Value, JsonError> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.skip_space();
    let value = reader.value(0)?;
    reader.skip_space();
    if reader.at != reader.bytes.len() {
        return Err(reader.fail("the text carries on after the value ended"));
    }
    Ok(value)
}

/// Write a value in the one spelling `docs/control-plane.md` declares.
///
/// No insignificant space anywhere, members in the order they are held, and
/// numbers exactly as their digits stand.
pub fn write(value: &Value) -> String {
    let mut out = String::new();
    put(value, &mut out);
    out
}

fn put(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Num(n) => out.push_str(n),
        Value::Str(s) => put_string(s, out),
        Value::Arr(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                put(item, out);
            }
            out.push(']');
        }
        Value::Obj(members) => {
            out.push('{');
            for (index, (key, item)) in members.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                put_string(key, out);
                out.push(':');
                put(item, out);
            }
            out.push('}');
        }
    }
}

/// Escape exactly the characters JSON requires, and nothing else.
///
/// `/` is not escaped, because escaping it is optional and a vector has to
/// pick one spelling. A character above U+007F is written as itself, in UTF-8:
/// the document declares the encoding, so `\u` escaping it would be a second
/// spelling of the same string.
fn put_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn fail(&self, detail: &str) -> JsonError {
        JsonError {
            at: self.at,
            detail: detail.to_string(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while let Some(b) = self.peek() {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => self.at += 1,
                _ => break,
            }
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, JsonError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            return Ok(value);
        }
        Err(self.fail("not a value"))
    }

    fn value(&mut self, depth: usize) -> Result<Value, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.fail(&format!(
                "nested deeper than the {} levels this reader follows",
                MAX_DEPTH
            )));
        }
        match self.peek() {
            None => Err(self.fail("the text ended where a value was expected")),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'"') => self.string().map(Value::Str),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            Some(_) => Err(self.fail("not a value")),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, JsonError> {
        self.at += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            self.skip_space();
            items.push(self.value(depth + 1)?);
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(self.fail("an array member is not followed by ',' or ']'")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, JsonError> {
        self.at += 1;
        let mut members: Vec<(String, Value)> = Vec::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Value::Obj(members));
        }
        loop {
            self.skip_space();
            if self.peek() != Some(b'"') {
                return Err(self.fail("an object member's name is not a string"));
            }
            let key = self.string()?;
            if members.iter().any(|(k, _)| *k == key) {
                return Err(self.fail(&format!(
                    "the field '{}' is given twice, and two spellings of one field have no \
                     agreed meaning",
                    key
                )));
            }
            self.skip_space();
            if self.peek() != Some(b':') {
                return Err(self.fail("an object member's name is not followed by ':'"));
            }
            self.at += 1;
            self.skip_space();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.skip_space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Obj(members));
                }
                _ => return Err(self.fail("an object member is not followed by ',' or '}'")),
            }
        }
    }

    fn number(&mut self) -> Result<Value, JsonError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err(self.fail("a number has no digits")),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.fail("a number's decimal point has no digits after it"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.at += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.fail("a number's exponent has no digits"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        // Every byte consumed above is ASCII, so this slice is on character
        // boundaries.
        let digits = std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| self.fail("a number is not text"))?;
        Ok(Value::Num(digits.to_string()))
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let b = match self.peek() {
                Some(b) => b,
                None => return Err(self.fail("a string was not closed")),
            };
            match b {
                b'"' => {
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.at += 1;
                    let escape = match self.peek() {
                        Some(e) => e,
                        None => return Err(self.fail("a string ended inside an escape")),
                    };
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.escaped_char()?),
                        _ => return Err(self.fail("not an escape this reader knows")),
                    }
                }
                b if b < 0x20 => {
                    return Err(self.fail(
                        "a raw control character in a string; JSON requires it to be escaped",
                    ))
                }
                _ => {
                    // Copy one whole character, however many bytes it is.
                    let rest = std::str::from_utf8(&self.bytes[self.at..])
                        .map_err(|_| self.fail("a string holds bytes that are not UTF-8"))?;
                    let c = rest.chars().next().unwrap_or('\u{fffd}');
                    out.push(c);
                    self.at += c.len_utf8();
                }
            }
        }
    }

    /// The character a `\u` escape names, following a surrogate pair where
    /// there is one.
    fn escaped_char(&mut self) -> Result<char, JsonError> {
        let first = self.hex4()?;
        if (0xD800..0xDC00).contains(&first) {
            if self.peek() != Some(b'\\') {
                return Err(self.fail("a leading surrogate is not followed by an escape"));
            }
            self.at += 1;
            if self.peek() != Some(b'u') {
                return Err(self.fail("a leading surrogate is not followed by a \\u escape"));
            }
            self.at += 1;
            let second = self.hex4()?;
            if !(0xDC00..0xE000).contains(&second) {
                return Err(self.fail("a leading surrogate is not followed by a trailing one"));
            }
            let combined = 0x1_0000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            return char::from_u32(combined).ok_or_else(|| self.fail("not a character"));
        }
        if (0xDC00..0xE000).contains(&first) {
            return Err(self.fail("a trailing surrogate with no leading one before it"));
        }
        char::from_u32(first).ok_or_else(|| self.fail("not a character"))
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut value = 0u32;
        for _ in 0..4 {
            let b = match self.peek() {
                Some(b) => b,
                None => return Err(self.fail("a \\u escape has fewer than four digits")),
            };
            let digit = match b {
                b'0'..=b'9' => u32::from(b - b'0'),
                b'a'..=b'f' => u32::from(b - b'a') + 10,
                b'A'..=b'F' => u32::from(b - b'A') + 10,
                _ => return Err(self.fail("a \\u escape has a digit that is not hexadecimal")),
            };
            value = value * 16 + digit;
            self.at += 1;
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_trip_keeps_the_order_and_the_digits() {
        let text = r#"{"v":1,"t":"volume","zone":"kitchen","volume":0.500}"#;
        let value = parse(text).expect("well formed");
        assert_eq!(write(&value), text, "the writer is the reader's inverse");
    }

    #[test]
    fn a_duplicate_field_is_refused_rather_than_resolved() {
        let err = parse(r#"{"zone":"a","zone":"b"}"#).unwrap_err();
        assert!(err.detail.contains("'zone' is given twice"), "{}", err);
    }

    #[test]
    fn trailing_content_is_refused_so_two_messages_cannot_read_as_one() {
        let err = parse(r#"{"v":1} {"v":1}"#).unwrap_err();
        assert!(err.detail.contains("carries on"), "{}", err);
    }

    #[test]
    fn the_things_json_is_not_are_refused() {
        for text in [
            "{",
            "}",
            "",
            "   ",
            "{\"a\"}",
            "{\"a\":}",
            "{\"a\":1,}",
            "[1,]",
            "[1 2]",
            "01",
            "1.",
            ".5",
            "+1",
            "1e",
            "\"unterminated",
            "'single'",
            "{'a':1}",
            "nul",
            "True",
            "NaN",
            "Infinity",
            "{\"a\":1}extra",
        ] {
            assert!(parse(text).is_err(), "{:?} is not JSON this reader takes", text);
        }
    }

    #[test]
    fn a_raw_control_character_in_a_string_is_refused() {
        let err = parse("{\"a\":\"x\ny\"}").unwrap_err();
        assert!(err.detail.contains("control character"), "{}", err);
    }

    #[test]
    fn nesting_past_the_ceiling_is_refused_rather_than_overflowing_the_stack() {
        let deep = format!("{}{}", "[".repeat(4_000), "]".repeat(4_000));
        let err = parse(&deep).unwrap_err();
        assert!(err.detail.contains("nested deeper"), "{}", err);
    }

    #[test]
    fn escapes_are_read_and_written_the_one_declared_way() {
        let value = parse(r#""a\"b\\c\ndAeé""#).unwrap();
        assert_eq!(value.as_str(), Some("a\"b\\c\ndAe\u{e9}"));
        assert_eq!(write(&value), "\"a\\\"b\\\\c\\ndAe\u{e9}\"");
        // A surrogate pair reads as the one character it names.
        let value = parse(r#""🎵""#).unwrap();
        assert_eq!(value.as_str(), Some("\u{1f3b5}"));
        // A lone surrogate does not.
        assert!(parse(r#""\ud83c""#).is_err());
        assert!(parse(r#""\udfb5""#).is_err());
    }

    #[test]
    fn a_control_character_is_written_escaped() {
        let value = Value::Str("a\u{01}b".to_string());
        assert_eq!(write(&value), "\"a\\u0001b\"");
    }
}
