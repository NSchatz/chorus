pub fn slash() -> &'static str {
    "// this is a string and not a comment"
}

pub fn block() -> &'static str {
    "/* nor is this, and it does not open anything */"
}

pub fn raw() -> &'static str {
    r"/* a raw string, still not a comment */"
}

pub fn raw_one_hash() -> &'static str {
    r#"and "// this" survives a quote inside it"#
}

pub fn raw_two_hashes() -> &'static str {
    r##"// two hashes, and "# does not close it"##
}

pub fn raw_three_hashes() -> &'static str {
    r###"/// three, and "## does not close it either"###
}

pub fn bytes() -> &'static [u8] {
    b"//! bytes are not prose"
}

pub fn raw_bytes() -> &'static [u8] {
    br#"/*! raw bytes are not prose either */"#
}

pub fn escaped() -> &'static str {
    "an escaped \" and then // still nothing"
}

pub fn slash_char() -> char {
    '/'
}

pub fn star_char() -> char {
    '*'
}

pub fn borrowed<'a>(value: &'a str) -> &'a str {
    value
}

pub fn labelled() -> usize {
    let mut count = 0;
    'outer: loop {
        count += 1;
        if count > 3 {
            break 'outer;
        }
    }
    count
}

pub fn raw_identifier() -> i64 {
    let r#type = 7;
    r#type
}
