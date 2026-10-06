//! The reader of a speaker design record: the plain JSON file the acoustics
//! package of the owner's shared Python library exports, which chorus reads
//! without that package (`docs/dsp.md`, "Design records").
//!
//! A record says what it is with a schema id and a version, holds each
//! crossover as biquads in chorus's own form (`b0 b1 b2 a1 a2`, `a0 = 1`) at a
//! stated sample rate, and carries the response its author computed at chosen
//! frequencies. [`DesignRecord::parse`] refuses a schema or a version it does
//! not know, a key it does not know, and a record whose biquads do not give
//! its own response points, so a record that was edited by hand is not run.
//!
//! Parsing text is not I/O: the caller reads the file. The JSON reader here is
//! the part of RFC 8259 a record uses (objects, arrays, strings, numbers and
//! the three literals), written out instead of taken as a dependency because
//! this library has none and the record's shape is fixed
//! (<https://www.rfc-editor.org/rfc/rfc8259>, read 2026-10-06).

use crate::biquad::{complex_mul, Coefficients};
use crate::crossover::{db_of, Lr4Design};
use crate::{MAX_RATE_HZ, MIN_RATE_HZ};

/// The schema id every record carries.
pub const SCHEMA: &str = "speaker-design-record";
/// The one schema version this reader knows.
pub const VERSION: u32 = 1;
/// How a version 1 record says its biquads are written.
pub const COEFFICIENT_FORM: &str = "b0 b1 b2 a1 a2, a0 = 1";

/// The deepest nesting the JSON reader follows (a record nests five deep).
const MAX_DEPTH: usize = 16;

/// Why a record was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordError {
    /// The text is not the JSON this reader accepts.
    Json(String),
    /// The `schema` is not [`SCHEMA`].
    UnknownSchema(String),
    /// The `version` is not [`VERSION`], written as the integer it is.
    UnknownVersion(String),
    /// A key is missing, unknown or of the wrong type, or a value is out of
    /// range.
    Shape(String),
    /// The biquads do not give one of the record's own response points.
    Response(String),
}

impl core::fmt::Display for RecordError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RecordError::Json(m) => write!(f, "not a JSON design record: {m}"),
            RecordError::UnknownSchema(s) => write!(f, "unknown schema {s} (known: {SCHEMA})"),
            RecordError::UnknownVersion(v) => {
                write!(f, "unknown schema version {v} (known: {VERSION})")
            }
            RecordError::Shape(m) => write!(f, "the record's shape: {m}"),
            RecordError::Response(m) => write!(f, "the record's response: {m}"),
        }
    }
}

impl std::error::Error for RecordError {}

/// The alignment of one crossover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossoverKind {
    /// Linkwitz-Riley 4th order: two sections per branch.
    Lr4,
    /// Linkwitz-Riley 2nd order: one section per branch, the inversion
    /// already in the high branch's `b0 b1 b2`.
    Lr2,
}

impl CrossoverKind {
    /// How many sections each branch has.
    pub fn sections(self) -> usize {
        match self {
            CrossoverKind::Lr4 => 2,
            CrossoverKind::Lr2 => 1,
        }
    }
}

/// One expected response point: `20 log10 |H|` of each branch and of the two
/// added, at `f_hz`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponsePoint {
    pub f_hz: f64,
    pub low_db: f64,
    pub high_db: f64,
    pub sum_db: f64,
}

/// One two-way split of a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Crossover {
    pub kind: CrossoverKind,
    /// The sampling rate the coefficients are for; they are wrong at any other.
    pub sample_rate_hz: f64,
    pub crossover_hz: f64,
    /// The low branch's sections, run in series in this order.
    pub low: Vec<Coefficients>,
    /// The high branch's sections, likewise.
    pub high: Vec<Coefficients>,
    /// The expected response points, never empty.
    pub response: Vec<ResponsePoint>,
}

impl Crossover {
    /// The low branch's complex response at `freq_hz`.
    pub fn response_low(&self, freq_hz: f64) -> (f64, f64) {
        cascade(&self.low, freq_hz, self.sample_rate_hz)
    }

    /// The high branch's complex response at `freq_hz`.
    pub fn response_high(&self, freq_hz: f64) -> (f64, f64) {
        cascade(&self.high, freq_hz, self.sample_rate_hz)
    }

    /// The response of the two branches added as they are.
    pub fn response_sum(&self, freq_hz: f64) -> (f64, f64) {
        let l = self.response_low(freq_hz);
        let h = self.response_high(freq_hz);
        (l.0 + h.0, l.1 + h.1)
    }

    /// The crossover as chorus's own LR4 design, when it is one: an `lr4`
    /// whose two sections per branch are the same section, which is what
    /// [`Lr4Design`] runs twice.
    pub fn lr4_design(&self) -> Option<Lr4Design> {
        if self.kind != CrossoverKind::Lr4
            || self.low[0] != self.low[1]
            || self.high[0] != self.high[1]
        {
            return None;
        }
        Some(Lr4Design {
            low: self.low[0],
            high: self.high[0],
        })
    }

    /// Holds the sections to every response point, within `tolerance_db`.
    pub fn check_response(&self, tolerance_db: f64) -> Result<(), RecordError> {
        for p in &self.response {
            let got = [
                ("low_db", db_of(self.response_low(p.f_hz)), p.low_db),
                ("high_db", db_of(self.response_high(p.f_hz)), p.high_db),
                ("sum_db", db_of(self.response_sum(p.f_hz)), p.sum_db),
            ];
            for (name, got, want) in got {
                let off = (got - want).abs();
                // A NaN (a level that could not be computed) is refused too.
                if off.is_nan() || off > tolerance_db {
                    return Err(RecordError::Response(format!(
                        "{name} at {} Hz is {got} dB from the biquads, {want} dB in the record",
                        p.f_hz
                    )));
                }
            }
        }
        Ok(())
    }
}

fn cascade(sections: &[Coefficients], freq_hz: f64, rate_hz: f64) -> (f64, f64) {
    sections.iter().fold((1.0, 0.0), |h, c| {
        complex_mul(h, c.response(freq_hz, rate_hz))
    })
}

/// A design record, version 1: a named set of crossovers.
#[derive(Clone, Debug, PartialEq)]
pub struct DesignRecord {
    /// The design's slug.
    pub name: String,
    /// How closely a consumer's own response must match the response points.
    pub tolerance_db: f64,
    /// The crossovers, never empty.
    pub crossovers: Vec<Crossover>,
}

impl DesignRecord {
    /// Reads a record's text. Refused: text that is not JSON, any schema but
    /// [`SCHEMA`], any version but [`VERSION`] (a version this reader does not
    /// know is never guessed at), a missing or unknown key, a value out of
    /// range, and biquads that do not give the record's own response points
    /// within its `tolerance_db`.
    pub fn parse(text: &str) -> Result<DesignRecord, RecordError> {
        let root = json::parse(text).map_err(RecordError::Json)?;
        let top = Object::of(&root, "the record")?;
        // The schema and the version are judged before anything else, so a
        // later version's new keys are reported as what they are.
        match top.find("schema") {
            Some(Value::Str(s)) if s == SCHEMA => {}
            Some(other) => return Err(RecordError::UnknownSchema(other.show())),
            None => return Err(RecordError::UnknownSchema("(none)".to_string())),
        }
        match top.find("version") {
            Some(Value::Num { value, integer }) if *integer && *value == f64::from(VERSION) => {}
            Some(other) => return Err(RecordError::UnknownVersion(other.show())),
            None => return Err(RecordError::UnknownVersion("(none)".to_string())),
        }
        top.only(&[
            "schema",
            "version",
            "name",
            "coefficient_form",
            "tolerance_db",
            "crossovers",
        ])?;
        let name = top.str("name")?.to_string();
        let slug = !name.is_empty()
            && name.len() <= 64
            && name.bytes().enumerate().all(|(i, b)| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || (i > 0 && b"._-".contains(&b))
            });
        if !slug {
            return Err(shape(format!("name {name:?} is not a slug")));
        }
        let form = top.str("coefficient_form")?;
        if form != COEFFICIENT_FORM {
            return Err(shape(format!(
                "coefficient_form {form:?} is not {COEFFICIENT_FORM:?}"
            )));
        }
        let tolerance_db = top.num("tolerance_db")?;
        if tolerance_db <= 0.0 {
            return Err(shape("tolerance_db is not above 0".to_string()));
        }
        let crossovers = top
            .list("crossovers")?
            .iter()
            .enumerate()
            .map(|(i, v)| crossover(v, &format!("crossovers[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        for c in &crossovers {
            c.check_response(tolerance_db)?;
        }
        Ok(DesignRecord {
            name,
            tolerance_db,
            crossovers,
        })
    }
}

fn shape(message: String) -> RecordError {
    RecordError::Shape(message)
}

fn crossover(v: &Value, at: &str) -> Result<Crossover, RecordError> {
    let o = Object::of(v, at)?;
    o.only(&[
        "kind",
        "sample_rate_hz",
        "crossover_hz",
        "low",
        "high",
        "response",
    ])?;
    let kind = match o.str("kind")? {
        "lr4" => CrossoverKind::Lr4,
        "lr2" => CrossoverKind::Lr2,
        other => return Err(shape(format!("{at}: unknown kind {other:?}"))),
    };
    let sample_rate_hz = o.num("sample_rate_hz")?;
    if !(f64::from(MIN_RATE_HZ)..=f64::from(MAX_RATE_HZ)).contains(&sample_rate_hz) {
        return Err(shape(format!(
            "{at}: sample_rate_hz {sample_rate_hz} is outside {MIN_RATE_HZ}..={MAX_RATE_HZ}"
        )));
    }
    let nyquist = sample_rate_hz / 2.0;
    let crossover_hz = o.num("crossover_hz")?;
    if !(crossover_hz > 0.0 && crossover_hz < nyquist) {
        return Err(shape(format!(
            "{at}: crossover_hz {crossover_hz} is not inside (0, rate/2)"
        )));
    }
    let branch = |key: &str| -> Result<Vec<Coefficients>, RecordError> {
        let sections = o
            .list(key)?
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let at = format!("{at}.{key}[{i}]");
                let b = Object::of(v, &at)?;
                b.only(&["b0", "b1", "b2", "a1", "a2"])?;
                Ok(Coefficients {
                    b0: b.num("b0")?,
                    b1: b.num("b1")?,
                    b2: b.num("b2")?,
                    a1: b.num("a1")?,
                    a2: b.num("a2")?,
                })
            })
            .collect::<Result<Vec<_>, RecordError>>()?;
        if sections.len() != kind.sections() {
            return Err(shape(format!(
                "{at}.{key}: {} sections, {} wanted",
                sections.len(),
                kind.sections()
            )));
        }
        Ok(sections)
    };
    let low = branch("low")?;
    let high = branch("high")?;
    let response = o
        .list("response")?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let at = format!("{at}.response[{i}]");
            let p = Object::of(v, &at)?;
            p.only(&["f_hz", "low_db", "high_db", "sum_db"])?;
            let f_hz = p.num("f_hz")?;
            if !(f_hz > 0.0 && f_hz < nyquist) {
                return Err(shape(format!(
                    "{at}: f_hz {f_hz} is not inside (0, rate/2)"
                )));
            }
            Ok(ResponsePoint {
                f_hz,
                low_db: p.num("low_db")?,
                high_db: p.num("high_db")?,
                sum_db: p.num("sum_db")?,
            })
        })
        .collect::<Result<Vec<_>, RecordError>>()?;
    if response.is_empty() {
        return Err(shape(format!("{at}.response is empty")));
    }
    Ok(Crossover {
        kind,
        sample_rate_hz,
        crossover_hz,
        low,
        high,
        response,
    })
}

use json::Value;

/// A JSON object read as a record's part: every key asked for must be there,
/// of the right type, and [`Object::only`] refuses a key nobody asks for.
struct Object<'a> {
    at: &'a str,
    members: &'a [(String, Value)],
}

impl<'a> Object<'a> {
    fn of(v: &'a Value, at: &'a str) -> Result<Object<'a>, RecordError> {
        match v {
            Value::Obj(members) => Ok(Object { at, members }),
            other => Err(shape(format!("{at} is {}, not an object", other.show()))),
        }
    }

    fn find(&self, key: &str) -> Option<&'a Value> {
        self.members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn need(&self, key: &str) -> Result<&'a Value, RecordError> {
        self.find(key)
            .ok_or_else(|| shape(format!("{}: no {key}", self.at)))
    }

    fn only(&self, keys: &[&str]) -> Result<(), RecordError> {
        match self
            .members
            .iter()
            .find(|(k, _)| !keys.contains(&k.as_str()))
        {
            Some((k, _)) => Err(shape(format!("{}: unknown key {k:?}", self.at))),
            None => Ok(()),
        }
    }

    fn str(&self, key: &str) -> Result<&'a str, RecordError> {
        match self.need(key)? {
            Value::Str(s) => Ok(s),
            other => Err(shape(format!(
                "{}: {key} is {}, not a string",
                self.at,
                other.show()
            ))),
        }
    }

    /// A finite number.
    fn num(&self, key: &str) -> Result<f64, RecordError> {
        match self.need(key)? {
            Value::Num { value, .. } if value.is_finite() => Ok(*value),
            other => Err(shape(format!(
                "{}: {key} is {}, not a number",
                self.at,
                other.show()
            ))),
        }
    }

    fn list(&self, key: &str) -> Result<&'a [Value], RecordError> {
        match self.need(key)? {
            Value::Arr(items) if !items.is_empty() => Ok(items),
            other => Err(shape(format!(
                "{}: {key} is {}, not a list with entries",
                self.at,
                other.show()
            ))),
        }
    }
}

/// The JSON reader: RFC 8259 values, an object's members kept in file order,
/// a repeated key refused.
mod json {
    use super::MAX_DEPTH;

    #[derive(Clone, Debug, PartialEq)]
    pub(super) enum Value {
        Null,
        Bool(bool),
        /// `integer` says the literal had no fraction and no exponent, which
        /// is how a schema version must be written.
        Num {
            value: f64,
            integer: bool,
        },
        Str(String),
        Arr(Vec<Value>),
        Obj(Vec<(String, Value)>),
    }

    impl Value {
        /// The value as an error message shows it: scalars whole, the rest by kind.
        pub(super) fn show(&self) -> String {
            match self {
                Value::Null => "null".to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Num { value, integer } if *integer => format!("{value:.0}"),
                Value::Num { value, .. } => format!("{value:?}"),
                Value::Str(s) => format!("{s:?}"),
                Value::Arr(_) => "a list".to_string(),
                Value::Obj(_) => "an object".to_string(),
            }
        }
    }

    pub(super) fn parse(text: &str) -> Result<Value, String> {
        let mut p = Parser {
            bytes: text.as_bytes(),
            at: 0,
        };
        let v = p.value(0)?;
        p.space();
        if p.at != p.bytes.len() {
            return Err(p.err("text after the value"));
        }
        Ok(v)
    }

    struct Parser<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl Parser<'_> {
        fn err(&self, what: &str) -> String {
            format!("{what} at byte {}", self.at)
        }

        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.at).copied()
        }

        fn space(&mut self) {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.at += 1;
            }
        }

        fn eat(&mut self, byte: u8) -> Result<(), String> {
            if self.peek() == Some(byte) {
                self.at += 1;
                Ok(())
            } else {
                Err(self.err(&format!("no '{}'", byte as char)))
            }
        }

        fn literal(&mut self, word: &str, v: Value) -> Result<Value, String> {
            if self.bytes[self.at..].starts_with(word.as_bytes()) {
                self.at += word.len();
                Ok(v)
            } else {
                Err(self.err("an unknown literal"))
            }
        }

        fn value(&mut self, depth: usize) -> Result<Value, String> {
            if depth > MAX_DEPTH {
                return Err(self.err("nested too deep"));
            }
            self.space();
            match self.peek() {
                Some(b'{') => self.object(depth),
                Some(b'[') => self.array(depth),
                Some(b'"') => self.string().map(Value::Str),
                Some(b't') => self.literal("true", Value::Bool(true)),
                Some(b'f') => self.literal("false", Value::Bool(false)),
                Some(b'n') => self.literal("null", Value::Null),
                Some(b'-' | b'0'..=b'9') => self.number(),
                Some(_) => Err(self.err("no value")),
                None => Err(self.err("the text ends")),
            }
        }

        fn object(&mut self, depth: usize) -> Result<Value, String> {
            self.eat(b'{')?;
            let mut members: Vec<(String, Value)> = Vec::new();
            self.space();
            if self.peek() == Some(b'}') {
                self.at += 1;
                return Ok(Value::Obj(members));
            }
            loop {
                self.space();
                let key = self.string()?;
                if members.iter().any(|(k, _)| *k == key) {
                    return Err(self.err(&format!("key {key:?} repeats")));
                }
                self.space();
                self.eat(b':')?;
                let v = self.value(depth + 1)?;
                members.push((key, v));
                self.space();
                if self.peek() == Some(b',') {
                    self.at += 1;
                } else {
                    self.eat(b'}')?;
                    return Ok(Value::Obj(members));
                }
            }
        }

        fn array(&mut self, depth: usize) -> Result<Value, String> {
            self.eat(b'[')?;
            let mut items = Vec::new();
            self.space();
            if self.peek() == Some(b']') {
                self.at += 1;
                return Ok(Value::Arr(items));
            }
            loop {
                items.push(self.value(depth + 1)?);
                self.space();
                if self.peek() == Some(b',') {
                    self.at += 1;
                } else {
                    self.eat(b']')?;
                    return Ok(Value::Arr(items));
                }
            }
        }

        fn string(&mut self) -> Result<String, String> {
            self.eat(b'"')?;
            let mut out = String::new();
            loop {
                let start = self.at;
                while !matches!(self.peek(), Some(b'"' | b'\\' | 0..=0x1f) | None) {
                    self.at += 1;
                }
                // The text is a &str and the run ends at an ASCII byte or at
                // the end, so it is whole characters.
                out.push_str(
                    core::str::from_utf8(&self.bytes[start..self.at])
                        .map_err(|_| self.err("a string is not UTF-8"))?,
                );
                match self.peek() {
                    Some(b'"') => {
                        self.at += 1;
                        return Ok(out);
                    }
                    Some(b'\\') => {
                        self.at += 1;
                        let c = self.peek().ok_or_else(|| self.err("the text ends"))?;
                        self.at += 1;
                        out.push(match c {
                            b'"' => '"',
                            b'\\' => '\\',
                            b'/' => '/',
                            b'b' => '\u{8}',
                            b'f' => '\u{c}',
                            b'n' => '\n',
                            b'r' => '\r',
                            b't' => '\t',
                            b'u' => self.unicode()?,
                            _ => return Err(self.err("an unknown escape")),
                        });
                    }
                    Some(_) => return Err(self.err("a control character in a string")),
                    None => return Err(self.err("the text ends in a string")),
                }
            }
        }

        fn hex4(&mut self) -> Result<u32, String> {
            let digits = self
                .bytes
                .get(self.at..self.at + 4)
                .and_then(|d| core::str::from_utf8(d).ok())
                .filter(|d| d.bytes().all(|b| b.is_ascii_hexdigit()))
                .ok_or_else(|| self.err("a \\u escape without four hex digits"))?;
            self.at += 4;
            u32::from_str_radix(digits, 16).map_err(|_| self.err("a \\u escape"))
        }

        /// The character of a `\u` escape, a surrogate pair included.
        fn unicode(&mut self) -> Result<char, String> {
            let first = self.hex4()?;
            let code = if (0xd800..0xdc00).contains(&first) {
                if !self.bytes[self.at..].starts_with(b"\\u") {
                    return Err(self.err("half a surrogate pair"));
                }
                self.at += 2;
                let second = self.hex4()?;
                if !(0xdc00..0xe000).contains(&second) {
                    return Err(self.err("half a surrogate pair"));
                }
                0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
            } else {
                first
            };
            char::from_u32(code).ok_or_else(|| self.err("half a surrogate pair"))
        }

        fn digits(&mut self) -> usize {
            let start = self.at;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
            self.at - start
        }

        fn number(&mut self) -> Result<Value, String> {
            let start = self.at;
            if self.peek() == Some(b'-') {
                self.at += 1;
            }
            let lead = self.peek();
            let whole = self.digits();
            if whole == 0 || (whole > 1 && lead == Some(b'0')) {
                return Err(self.err("a malformed number"));
            }
            let mut integer = true;
            if self.peek() == Some(b'.') {
                integer = false;
                self.at += 1;
                if self.digits() == 0 {
                    return Err(self.err("a malformed number"));
                }
            }
            if matches!(self.peek(), Some(b'e' | b'E')) {
                integer = false;
                self.at += 1;
                if matches!(self.peek(), Some(b'+' | b'-')) {
                    self.at += 1;
                }
                if self.digits() == 0 {
                    return Err(self.err("a malformed number"));
                }
            }
            let literal = core::str::from_utf8(&self.bytes[start..self.at])
                .map_err(|_| self.err("a malformed number"))?;
            let value: f64 = literal
                .parse()
                .map_err(|_| self.err("a malformed number"))?;
            Ok(Value::Num { value, integer })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::json::{parse, Value};
    use super::*;

    #[test]
    fn the_json_reader_reads_every_kind_of_value() {
        let v = parse(r#" {"a": [1, -2.5e1, 0], "b": "x\n\u00e9\ud83d\ude00\"", "c": [true, false, null], "d": {}} "#)
            .unwrap();
        let Value::Obj(m) = v else {
            panic!("an object")
        };
        assert_eq!(
            m[0].1,
            Value::Arr(vec![
                Value::Num {
                    value: 1.0,
                    integer: true
                },
                Value::Num {
                    value: -25.0,
                    integer: false
                },
                Value::Num {
                    value: 0.0,
                    integer: true
                },
            ])
        );
        assert_eq!(m[1].1, Value::Str("x\n\u{e9}\u{1f600}\"".to_string()));
        assert_eq!(
            m[2].1,
            Value::Arr(vec![Value::Bool(true), Value::Bool(false), Value::Null])
        );
        assert_eq!(m[3].1, Value::Obj(vec![]));
    }

    #[test]
    fn the_json_reader_refuses_what_is_not_json() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\":1,}",
            "{\"a\" 1}",
            "01",
            "1.",
            "-",
            "1e",
            "+1",
            ".5",
            "nul",
            "\"a",
            "\"\\x\"",
            "\"\\u12\"",
            "\"\\ud800\"",
            "\"a\nb\"",
            "1 2",
            "{\"a\":1,\"a\":2}",
            "{a:1}",
            "NaN",
            "Infinity",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} was read");
        }
        let deep = "[".repeat(MAX_DEPTH + 2) + &"]".repeat(MAX_DEPTH + 2);
        assert!(parse(&deep).is_err(), "nesting has no limit");
    }

    #[test]
    fn a_number_too_large_for_a_double_is_not_a_number() {
        let text = r#"{"schema":"speaker-design-record","version":1,"name":"x","coefficient_form":"b0 b1 b2 a1 a2, a0 = 1","tolerance_db":1e999,"crossovers":[]}"#;
        assert!(matches!(
            DesignRecord::parse(text),
            Err(RecordError::Shape(_))
        ));
    }
}
