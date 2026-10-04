//! The model manifest: the small JSON file microWakeWord publishes beside
//! each model, with the phrase and the detection parameters.
//!
//! The reader below is a complete JSON parser of about a hundred lines, kept
//! here so the crate has no dependency; it holds strings, numbers, objects,
//! arrays and literals, and nothing about it is specific to the manifest.

use crate::Error;

/// What a manifest says about its model.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    /// The phrase the model detects, as people say it ("Okay Nabu").
    pub phrase: String,
    /// The mean probability a detection must exceed, in `(0, 1)`.
    pub probability_cutoff: f32,
    /// How many consecutive inferences the mean is taken over.
    pub sliding_window_size: usize,
}

#[derive(Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
    depth: usize,
}

const BAD: Error = Error::Manifest("not valid JSON");

impl Parser<'_> {
    fn space(&mut self) {
        while matches!(self.text.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, word: &[u8]) -> Result<(), Error> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(())
        } else {
            Err(BAD)
        }
    }

    fn value(&mut self) -> Result<Json, Error> {
        self.space();
        self.depth += 1;
        if self.depth > 32 {
            return Err(Error::Manifest("nested too deeply"));
        }
        let v = match self.text.get(self.at).ok_or(BAD)? {
            b'n' => self.eat(b"null").map(|()| Json::Null)?,
            b't' => self.eat(b"true").map(|()| Json::Bool(true))?,
            b'f' => self.eat(b"false").map(|()| Json::Bool(false))?,
            b'"' => Json::String(self.string()?),
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.text.get(self.at) == Some(&b']') {
                    self.at += 1;
                } else {
                    loop {
                        items.push(self.value()?);
                        self.space();
                        match self.text.get(self.at) {
                            Some(b',') => self.at += 1,
                            Some(b']') => {
                                self.at += 1;
                                break;
                            }
                            _ => return Err(BAD),
                        }
                    }
                }
                Json::Array(items)
            }
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                self.space();
                if self.text.get(self.at) == Some(&b'}') {
                    self.at += 1;
                } else {
                    loop {
                        self.space();
                        let key = self.string()?;
                        self.space();
                        self.eat(b":")?;
                        fields.push((key, self.value()?));
                        self.space();
                        match self.text.get(self.at) {
                            Some(b',') => self.at += 1,
                            Some(b'}') => {
                                self.at += 1;
                                break;
                            }
                            _ => return Err(BAD),
                        }
                    }
                }
                Json::Object(fields)
            }
            _ => {
                let start = self.at;
                while matches!(
                    self.text.get(self.at),
                    Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                ) {
                    self.at += 1;
                }
                let s = std::str::from_utf8(&self.text[start..self.at]).map_err(|_| BAD)?;
                Json::Number(s.parse().map_err(|_| BAD)?)
            }
        };
        self.depth -= 1;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, Error> {
        if self.text.get(self.at) != Some(&b'"') {
            return Err(BAD);
        }
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let b = *self.text.get(self.at).ok_or(BAD)?;
            self.at += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let e = *self.text.get(self.at).ok_or(BAD)?;
                    self.at += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = self
                                .text
                                .get(self.at..self.at + 4)
                                .and_then(|h| std::str::from_utf8(h).ok())
                                .ok_or(BAD)?;
                            let code = u32::from_str_radix(hex, 16).map_err(|_| BAD)?;
                            self.at += 4;
                            // A surrogate half has no character of its own; the manifests hold none.
                            let c = char::from_u32(code)
                                .ok_or(Error::Manifest("a surrogate escape"))?;
                            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        _ => return Err(BAD),
                    }
                }
                0..=0x1f => return Err(BAD),
                _ => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| BAD)
    }
}

fn parse(text: &str) -> Result<Json, Error> {
    let mut p = Parser {
        text: text.as_bytes(),
        at: 0,
        depth: 0,
    };
    let v = p.value()?;
    p.space();
    if p.at != p.text.len() {
        return Err(BAD);
    }
    Ok(v)
}

impl Manifest {
    /// Reads a microWakeWord manifest (`"type": "micro"`, version 2).
    ///
    /// Version 2 fixes the feature step at 10 ms, which is the only step the
    /// frontend here produces; a manifest that asks for another is refused.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let json = parse(text)?;
        if json.get("type") != Some(&Json::String("micro".into())) {
            return Err(Error::Manifest(
                "not a microWakeWord manifest (type is not \"micro\")",
            ));
        }
        let Some(Json::String(phrase)) = json.get("wake_word") else {
            return Err(Error::Manifest("no wake_word"));
        };
        let micro = json
            .get("micro")
            .ok_or(Error::Manifest("no micro section"))?;
        let number = |key: &'static str| match micro.get(key) {
            Some(Json::Number(n)) => Ok(*n),
            _ => Err(Error::Manifest(
                "a missing or non-numeric field in the micro section",
            )),
        };
        let cutoff = number("probability_cutoff")?;
        let window = number("sliding_window_size")?;
        if !(cutoff > 0.0 && cutoff < 1.0)
            || !(1.0..=1000.0).contains(&window)
            || window.fract() != 0.0
        {
            return Err(Error::Manifest(
                "a cutoff outside (0, 1) or a window outside 1 to 1000",
            ));
        }
        if micro.get("feature_step_size").is_some() && number("feature_step_size")? != 10.0 {
            return Err(Error::Manifest("a feature step other than 10 ms"));
        }
        if phrase.is_empty() {
            return Err(Error::Manifest("an empty wake_word"));
        }
        Ok(Self {
            phrase: phrase.clone(),
            probability_cutoff: cutoff as f32,
            sliding_window_size: window as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_values_parse() {
        let v = parse(r#" {"a": [1, -2.5e1, true, null, "x\n\u00e9"], "b": {}} "#).unwrap();
        assert_eq!(
            v.get("a"),
            Some(&Json::Array(vec![
                Json::Number(1.0),
                Json::Number(-25.0),
                Json::Bool(true),
                Json::Null,
                Json::String("x\n\u{e9}".into())
            ]))
        );
        assert_eq!(v.get("b"), Some(&Json::Object(vec![])));
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "nul",
            "1 2",
            "\"\\x\"",
            "\"open",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(parse(&"[".repeat(100)).is_err());
    }

    #[test]
    fn a_manifest_is_checked() {
        let good = r#"{"type":"micro","wake_word":"Okay Nabu","micro":{"probability_cutoff":0.97,"feature_step_size":10,"sliding_window_size":5}}"#;
        let m = Manifest::from_json(good).unwrap();
        assert_eq!(
            m,
            Manifest {
                phrase: "Okay Nabu".into(),
                probability_cutoff: 0.97,
                sliding_window_size: 5
            }
        );
        for (from, to) in [
            ("\"micro\",", "\"other\","),
            ("0.97", "1.5"),
            ("\"feature_step_size\":10", "\"feature_step_size\":20"),
            (":5}", ":0}"),
            ("Okay Nabu", ""),
        ] {
            assert!(
                Manifest::from_json(&good.replace(from, to)).is_err(),
                "{from} -> {to}"
            );
        }
    }
}
