//! XML as UPnP uses it: two escape functions and a small tolerant reader.
//!
//! What a renderer reads is three small dialects from untrusted peers: the
//! SOAP envelope of an action (UDA11 section 3.2.1), the DIDL-Lite document
//! inside an argument (CDS1), and, on the control point side, descriptions
//! and event bodies. None of them needs a document type, an entity
//! declaration, or anything else from XML's DTD half, and that half is where
//! XML's classic attacks live. So the reader here has no DTD half at all:
//!
//! - a `<!DOCTYPE` (or any other `<!` declaration that is not a comment or a
//!   CDATA section) ends the parse with [`XmlError::Doctype`];
//! - the only entity references ever replaced are the five XML 1.0 section
//!   4.6 predefines and numeric character references; any other `&name;` is
//!   left as the characters it is, so there is nothing to expand and nothing
//!   to fetch (an external entity, "XXE", or a "billion laughs" expansion
//!   cannot be expressed);
//! - input size, nesting depth and attributes per element are bounded by
//!   [`Limits`] before any work is done on them.
//!
//! It is tolerant where control points are sloppy: any namespace prefix (the
//! reader resolves prefixes to URIs, and callers may also match by local name
//! alone), a missing XML declaration, a byte order mark, comments, processing
//! instructions, text after the root element. It is strict about nesting: an
//! end tag that does not match its start tag is an error, because guessing
//! there would hand an action the wrong arguments.

/// Escapes text for element content: `&`, `<` and `>` (UDA11 section 3.2.1:
/// "the text MUST be escaped" per XML 1.0 section 2.4), and a carriage
/// return as a character reference, because XML 1.0 section 2.11 has a
/// parser turn a literal one into a line feed and the value would not come
/// back as it was sent.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escapes text for a double-quoted attribute value: what [`escape_text`]
/// escapes, plus `"`, and tab and line feed as character references, because
/// XML 1.0 section 3.3.3 has a parser normalise literal white space in an
/// attribute value to spaces. A LastChange event carries a whole DIDL-Lite
/// document in a `val` attribute, line breaks included.
pub fn escape_attr(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
    out
}

/// Replaces the five predefined entity references (XML 1.0 section 4.6) and
/// numeric character references (section 4.1, decimal and hexadecimal) once.
/// Anything else that starts with `&` is kept as written: an undeclared
/// entity, a reference to a code point that is not a character, a bare
/// ampersand in a URL from a control point that forgot to escape it.
pub fn unescape(text: &str) -> String {
    let Some(first) = text.find('&') else {
        return text.to_string();
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..first]);
    let mut rest = &text[first..];
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        // The longest reference worth reading is `&#x10FFFF;`.
        let end = rest
            .char_indices()
            .take(12)
            .find(|(_, c)| *c == ';')
            .map(|(i, _)| i);
        let replaced = end.and_then(|end| {
            let name = &rest[1..end];
            let c = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let digits = name.strip_prefix('#')?;
                    let code = match digits.strip_prefix(['x', 'X']) {
                        Some(hex) if !hex.is_empty() => u32::from_str_radix(hex, 16).ok()?,
                        Some(_) => return None,
                        None if !digits.is_empty()
                            && digits.bytes().all(|b| b.is_ascii_digit()) =>
                        {
                            digits.parse().ok()?
                        }
                        None => return None,
                    };
                    // XML 1.0 section 2.2: #x0 is not a character.
                    if code == 0 {
                        return None;
                    }
                    char::from_u32(code)?
                }
            };
            Some((c, end))
        });
        match replaced {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The hard limits of one parse. They are checked before the work they
/// bound: the size before the first byte is read, the depth before an
/// element is opened, the attribute count as attributes are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The largest input, in bytes.
    pub max_bytes: usize,
    /// The deepest element nesting; the root is depth 1.
    pub max_depth: usize,
    /// The most attributes on one element, namespace declarations included.
    pub max_attributes: usize,
}

impl Limits {
    /// The limits for everything a renderer reads from a control point:
    /// 64 KiB, 32 levels, 32 attributes. chorus's own choice: a SOAP action
    /// with a DIDL-Lite argument is a few kilobytes and nests under ten
    /// levels.
    pub const DEFAULT: Limits = Limits {
        max_bytes: 64 * 1024,
        max_depth: 32,
        max_attributes: 32,
    };
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::DEFAULT
    }
}

/// Why a document was not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XmlError {
    /// The input is larger than [`Limits::max_bytes`].
    TooLarge,
    /// An element is nested deeper than [`Limits::max_depth`].
    TooDeep,
    /// An element has more attributes than [`Limits::max_attributes`].
    TooManyAttributes,
    /// The input holds a `<!DOCTYPE` or another markup declaration (an
    /// entity, element or attribute-list declaration). Refused always: see
    /// the module documentation.
    Doctype,
    /// The input is not well formed; the text says where it stopped making
    /// sense.
    Malformed(&'static str),
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XmlError::TooLarge => f.write_str("the document is larger than the limit"),
            XmlError::TooDeep => f.write_str("the document nests deeper than the limit"),
            XmlError::TooManyAttributes => f.write_str("an element has too many attributes"),
            XmlError::Doctype => f.write_str("the document holds a DOCTYPE or markup declaration"),
            XmlError::Malformed(what) => write!(f, "the document is not well formed: {what}"),
        }
    }
}

impl std::error::Error for XmlError {}

/// One attribute, its value unescaped once. Namespace declarations (`xmlns`
/// and `xmlns:p`) are consumed by the reader and are not attributes here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    /// The prefix before the colon, or empty.
    pub prefix: String,
    /// The name after the colon, or the whole name.
    pub local: String,
    /// The value.
    pub value: String,
}

/// A start tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// The prefix before the colon, or empty.
    pub prefix: String,
    /// The name after the colon, or the whole name.
    pub local: String,
    /// The namespace URI the prefix (or the default namespace) is bound to
    /// at this element, if any.
    pub namespace: Option<String>,
    /// The attributes in document order.
    pub attributes: Vec<Attribute>,
}

/// What the reader hands out, one at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A start tag. An empty-element tag (`<a/>`) is a start followed at
    /// once by an end.
    Start(Element),
    /// An end tag, by its local name.
    End(String),
    /// Character data, unescaped once, or the content of a CDATA section as
    /// written.
    Text(String),
}

struct Open<'a> {
    raw_name: &'a str,
    local: &'a str,
    ns_mark: usize,
}

/// The pull reader. [`Reader::next`] returns events until the root element
/// closes, then `None`; whatever follows the root is not read.
pub struct Reader<'a> {
    src: &'a str,
    pos: usize,
    token_start: usize,
    limits: Limits,
    open: Vec<Open<'a>>,
    namespaces: Vec<(&'a str, String)>,
    pending_end: bool,
    root_seen: bool,
}

fn split_name(raw: &str) -> (&str, &str) {
    match raw.split_once(':') {
        Some((p, l)) if !p.is_empty() && !l.is_empty() => (p, l),
        _ => ("", raw),
    }
}

impl<'a> Reader<'a> {
    /// A reader over `src`, refused at once when `src` is over the size
    /// limit.
    pub fn new(src: &'a str, limits: Limits) -> Result<Reader<'a>, XmlError> {
        if src.len() > limits.max_bytes {
            return Err(XmlError::TooLarge);
        }
        // A byte order mark is not content (XML 1.0 section 4.3.3).
        let pos = if src.starts_with('\u{feff}') { 3 } else { 0 };
        Ok(Reader {
            src,
            pos,
            token_start: pos,
            limits,
            open: Vec::new(),
            namespaces: Vec::new(),
            pending_end: false,
            root_seen: false,
        })
    }

    /// The byte offset just past the last event's markup: after a
    /// [`Event::Start`] it is where the element's content begins.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// The byte offset where the last event's markup began: at an
    /// [`Event::End`] it is where the element's content ended.
    pub fn token_start(&self) -> usize {
        self.token_start
    }

    /// How many elements are open.
    pub fn depth(&self) -> usize {
        self.open.len()
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn skip_past(&mut self, end: &str, what: &'static str) -> Result<&'a str, XmlError> {
        let rest = self.rest();
        let at = rest.find(end).ok_or(XmlError::Malformed(what))?;
        self.pos += at + end.len();
        Ok(&rest[..at])
    }

    fn resolve(&self, prefix: &str) -> Option<String> {
        self.namespaces
            .iter()
            .rev()
            .find(|(p, _)| *p == prefix)
            .map(|(_, uri)| uri.clone())
            .filter(|uri| !uri.is_empty())
    }

    fn close(&mut self) -> Event {
        let open = self.open.pop().expect("an element is open");
        self.namespaces.truncate(open.ns_mark);
        Event::End(open.local.to_string())
    }

    /// The next event, `Ok(None)` once the root element has closed.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Event>, XmlError> {
        if self.pending_end {
            self.pending_end = false;
            self.token_start = self.pos;
            return Ok(Some(self.close()));
        }
        loop {
            if self.root_seen && self.open.is_empty() {
                return Ok(None);
            }
            let rest = self.rest();
            self.token_start = self.pos;
            if rest.is_empty() {
                return Err(XmlError::Malformed(if self.root_seen {
                    "the document ends inside an element"
                } else {
                    "there is no root element"
                }));
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                let text = &rest[..end];
                self.pos += end;
                if self.open.is_empty() {
                    if text.trim().is_empty() {
                        continue;
                    }
                    return Err(XmlError::Malformed("text before the root element"));
                }
                return Ok(Some(Event::Text(unescape(text))));
            }
            if rest.starts_with("<!--") {
                self.pos += 4;
                self.skip_past("-->", "a comment is not closed")?;
                continue;
            }
            if rest.starts_with("<![CDATA[") {
                if self.open.is_empty() {
                    return Err(XmlError::Malformed("a CDATA section outside the root"));
                }
                self.pos += 9;
                let text = self.skip_past("]]>", "a CDATA section is not closed")?;
                return Ok(Some(Event::Text(text.to_string())));
            }
            if rest.starts_with("<!") {
                // DOCTYPE, ENTITY, ELEMENT, ATTLIST, NOTATION: the DTD half.
                return Err(XmlError::Doctype);
            }
            if rest.starts_with("<?") {
                self.pos += 2;
                self.skip_past("?>", "a processing instruction is not closed")?;
                continue;
            }
            if rest.starts_with("</") {
                self.pos += 2;
                let name = self.skip_past(">", "an end tag is not closed")?.trim_end();
                let Some(open) = self.open.last() else {
                    return Err(XmlError::Malformed("an end tag with nothing open"));
                };
                if open.raw_name != name {
                    return Err(XmlError::Malformed(
                        "an end tag does not match its start tag",
                    ));
                }
                return Ok(Some(self.close()));
            }
            return self.start_tag().map(Some);
        }
    }

    fn start_tag(&mut self) -> Result<Event, XmlError> {
        self.pos += 1;
        let rest = self.rest();
        let name_len = rest
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .ok_or(XmlError::Malformed("a start tag is not closed"))?;
        let raw_name = &rest[..name_len];
        if raw_name.is_empty() || raw_name.contains(['<', '&', '"', '\'', '=']) {
            return Err(XmlError::Malformed("a tag has no name"));
        }
        self.pos += name_len;
        if self.open.len() >= self.limits.max_depth {
            return Err(XmlError::TooDeep);
        }
        let ns_mark = self.namespaces.len();
        let mut attributes: Vec<Attribute> = Vec::new();
        let mut count = 0usize;
        let empty = loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.pos += rest.len() - trimmed.len();
            if trimmed.starts_with("/>") {
                self.pos += 2;
                break true;
            }
            if trimmed.starts_with('>') {
                self.pos += 1;
                break false;
            }
            let eq = trimmed
                .find(['=', '>', '<'])
                .filter(|i| trimmed.as_bytes()[*i] == b'=')
                .ok_or(XmlError::Malformed("an attribute has no value"))?;
            let name = trimmed[..eq].trim_end();
            if name.is_empty() || name.contains(char::is_whitespace) {
                return Err(XmlError::Malformed("an attribute has no name"));
            }
            let after = trimmed[eq + 1..].trim_start();
            let quote = after
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
                .ok_or(XmlError::Malformed("an attribute value is not quoted"))?;
            let value_end = after[1..]
                .find(quote)
                .ok_or(XmlError::Malformed("an attribute value is not closed"))?;
            let raw_value = &after[1..1 + value_end];
            let consumed = trimmed.len() - after.len() + value_end + 2;
            self.pos += consumed;
            count += 1;
            if count > self.limits.max_attributes {
                return Err(XmlError::TooManyAttributes);
            }
            let value = unescape(raw_value);
            if name == "xmlns" {
                self.namespaces.push(("", value));
            } else if let Some(prefix) = name.strip_prefix("xmlns:") {
                self.namespaces.push((prefix, value));
            } else {
                let (prefix, local) = split_name(name);
                attributes.push(Attribute {
                    prefix: prefix.to_string(),
                    local: local.to_string(),
                    value,
                });
            }
        };
        let (prefix, local) = split_name(raw_name);
        let namespace = self.resolve(prefix);
        self.open.push(Open {
            raw_name,
            local,
            ns_mark,
        });
        self.root_seen = true;
        self.pending_end = empty;
        Ok(Event::Start(Element {
            prefix: prefix.to_string(),
            local: local.to_string(),
            namespace,
            attributes,
        }))
    }
}

/// An element with everything under it: what [`parse`] builds. Small by
/// construction, since the input is bounded by [`Limits`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// The prefix as written, or empty.
    pub prefix: String,
    /// The local name.
    pub local: String,
    /// The namespace URI in force, if any.
    pub namespace: Option<String>,
    /// The attributes in document order.
    pub attributes: Vec<Attribute>,
    /// The child elements in document order.
    pub children: Vec<Node>,
    /// The element's own character data (not its children's), concatenated,
    /// unescaped once.
    pub text: String,
    /// The element's content exactly as written, markup included.
    pub inner_xml: String,
}

impl Node {
    /// The first child of that local name, whatever its prefix.
    pub fn child(&self, local: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.local == local)
    }

    /// Every child of that local name, whatever its prefix.
    pub fn children_named<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |c| c.local == local)
    }

    /// The first child of that local name in that namespace.
    pub fn child_ns(&self, namespace: &str, local: &str) -> Option<&Node> {
        self.children
            .iter()
            .find(|c| c.local == local && c.namespace.as_deref() == Some(namespace))
    }

    /// The first attribute of that local name, whatever its prefix.
    pub fn attr(&self, local: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| a.local == local)
            .map(|a| a.value.as_str())
    }

    /// The first element of that local name at or under this one, depth
    /// first.
    pub fn find(&self, local: &str) -> Option<&Node> {
        if self.local == local {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(local))
    }

    /// The text of the first child of that local name, trimmed.
    pub fn child_text(&self, local: &str) -> Option<&str> {
        self.child(local).map(|c| c.text.trim())
    }
}

/// Reads a whole document into its root [`Node`].
pub fn parse(src: &str, limits: Limits) -> Result<Node, XmlError> {
    let mut reader = Reader::new(src, limits)?;
    // The nodes being built, innermost last, each with where its content
    // began.
    let mut stack: Vec<(Node, usize)> = Vec::new();
    loop {
        match reader.next()? {
            Some(Event::Start(e)) => {
                stack.push((
                    Node {
                        prefix: e.prefix,
                        local: e.local,
                        namespace: e.namespace,
                        attributes: e.attributes,
                        children: Vec::new(),
                        text: String::new(),
                        inner_xml: String::new(),
                    },
                    reader.position(),
                ));
            }
            Some(Event::Text(t)) => {
                if let Some((node, _)) = stack.last_mut() {
                    node.text.push_str(&t);
                }
            }
            Some(Event::End(_)) => {
                let (mut node, start) = stack.pop().expect("an end follows a start");
                let end = reader.token_start().max(start);
                node.inner_xml = src[start..end].to_string();
                match stack.last_mut() {
                    Some((parent, _)) => parent.children.push(node),
                    None => return Ok(node),
                }
            }
            None => return Err(XmlError::Malformed("there is no root element")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRICKY: &str = "A & B <\"x\"> \u{e9}";

    #[test]
    fn text_and_attribute_escaping_round_trip() {
        assert_eq!(escape_text(TRICKY), "A &amp; B &lt;\"x\"&gt; \u{e9}");
        assert_eq!(
            escape_attr(TRICKY),
            "A &amp; B &lt;&quot;x&quot;&gt; \u{e9}"
        );
        assert_eq!(unescape(&escape_text(TRICKY)), TRICKY);
        assert_eq!(unescape(&escape_attr(TRICKY)), TRICKY);
        // Twice escaped, twice unescaped: what LastChange does to metadata.
        let twice = escape_text(&escape_attr(TRICKY));
        assert_eq!(unescape(&unescape(&twice)), TRICKY);
        assert_ne!(unescape(&twice), TRICKY);
        // White space that a parser would normalise is written as references.
        assert_eq!(escape_attr("a\tb\nc\rd"), "a&#9;b&#10;c&#13;d");
        assert_eq!(escape_text("a\tb\nc\rd"), "a\tb\nc&#13;d");
    }

    #[test]
    fn unescape_reads_the_five_names_and_numeric_references_only() {
        let cases = [
            ("&amp;&lt;&gt;&quot;&apos;", "&<>\"'"),
            ("&#38;&#x26;&#X26;", "&&&"),
            ("&#233;&#xe9;&#x1F600;", "\u{e9}\u{e9}\u{1F600}"),
            ("a&b", "a&b"),
            ("a&b;c", "a&b;c"),
            ("&nbsp;", "&nbsp;"),
            ("&lol9;", "&lol9;"),
            ("&#0;", "&#0;"),
            ("&#xD800;", "&#xD800;"),
            ("&#x110000;", "&#x110000;"),
            ("&#;", "&#;"),
            ("&#x;", "&#x;"),
            ("&#12a;", "&#12a;"),
            ("&", "&"),
            ("&&amp;", "&&"),
            ("&amp;amp;", "&amp;"),
            ("x=1&y=2&amp;z=3", "x=1&y=2&z=3"),
            ("", ""),
        ];
        for (input, expected) in cases {
            assert_eq!(unescape(input), expected, "{input}");
        }
    }

    fn events(src: &str) -> Result<Vec<Event>, XmlError> {
        let mut r = Reader::new(src, Limits::DEFAULT)?;
        let mut out = Vec::new();
        while let Some(e) = r.next()? {
            out.push(e);
        }
        Ok(out)
    }

    #[test]
    fn the_reader_yields_elements_attributes_text_and_cdata() {
        let src = "\u{feff}<?xml version=\"1.0\"?>\n<!-- c -->\n<a x='1' y = \"2 &amp; 3\"><b/>t&lt;<![CDATA[<raw&amp;>]]><?pi?><!-- c --></a> trailing <junk";
        let got = events(src).unwrap();
        assert_eq!(got.len(), 6);
        let Event::Start(a) = &got[0] else { panic!() };
        assert_eq!(a.local, "a");
        assert_eq!(a.attributes.len(), 2);
        assert_eq!(a.attributes[1].value, "2 & 3");
        assert_eq!(
            got[1],
            Event::Start(Element {
                prefix: String::new(),
                local: "b".into(),
                namespace: None,
                attributes: vec![],
            })
        );
        assert_eq!(got[2], Event::End("b".into()));
        assert_eq!(got[3], Event::Text("t<".into()));
        assert_eq!(got[4], Event::Text("<raw&amp;>".into()));
        assert_eq!(got[5], Event::End("a".into()));
    }

    #[test]
    fn prefixes_resolve_to_namespaces_and_scopes_end_with_their_element() {
        let src = r#"<s:Envelope xmlns:s="urn:soap"><s:Body><u:Play xmlns:u="urn:avt"><InstanceID>0</InstanceID></u:Play><Play xmlns="urn:other"><X/></Play><u:Late/></s:Body></s:Envelope>"#;
        let root = parse(src, Limits::DEFAULT).unwrap();
        assert_eq!(root.namespace.as_deref(), Some("urn:soap"));
        let body = root.child("Body").unwrap();
        assert_eq!(body.prefix, "s");
        let play = &body.children[0];
        assert_eq!(play.namespace.as_deref(), Some("urn:avt"));
        // An unprefixed child of a prefixed element is in no namespace.
        assert_eq!(play.children[0].namespace, None);
        assert_eq!(play.child_text("InstanceID"), Some("0"));
        // A default namespace applies to the element and its children.
        let other = &body.children[1];
        assert_eq!(other.namespace.as_deref(), Some("urn:other"));
        assert_eq!(other.children[0].namespace.as_deref(), Some("urn:other"));
        // The `u` prefix went out of scope with the element that declared it.
        assert_eq!(body.children[2].prefix, "u");
        assert_eq!(body.children[2].namespace, None);
        assert_eq!(body.child_ns("urn:other", "Play"), Some(other));
        assert!(root.find("X").is_some());
        assert!(root.find("Nope").is_none());
    }

    #[test]
    fn a_node_keeps_its_content_as_written() {
        let root = parse("<a><b x=\"1\">t &amp; <c/> u</b><d/></a>", Limits::DEFAULT).unwrap();
        let b = root.child("b").unwrap();
        assert_eq!(b.inner_xml, "t &amp; <c/> u");
        assert_eq!(b.text, "t &  u");
        assert_eq!(b.attr("x"), Some("1"));
        assert_eq!(root.child("d").unwrap().inner_xml, "");
        assert_eq!(root.inner_xml, "<b x=\"1\">t &amp; <c/> u</b><d/>");
        assert_eq!(root.children_named("b").count(), 1);
    }

    /// The refusals that make XXE and entity expansion impossible: a DOCTYPE
    /// in any position and any other declaration end the parse, and an
    /// entity reference that only a DTD could define stays text.
    #[test]
    fn doctype_and_entity_declarations_are_refused() {
        let hostile = [
            "<!DOCTYPE a><a/>",
            "<?xml version=\"1.0\"?><!DOCTYPE a SYSTEM \"file:///etc/passwd\"><a/>",
            "<!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;&lol;&lol;\">]><lolz>&lol2;</lolz>",
            "<!doctype a><a/>",
            "<a><!DOCTYPE b></a>",
            "<a><!ENTITY x SYSTEM \"http://192.0.2.1/x\"></a>",
            "<!ENTITY x \"y\"><a/>",
            "<a><!ELEMENT b ANY></a>",
            "<a><!ATTLIST b c CDATA #IMPLIED></a>",
            "<a><![INCLUDE[ x ]]></a>",
        ];
        for src in hostile {
            assert_eq!(parse(src, Limits::DEFAULT), Err(XmlError::Doctype), "{src}");
        }
        let root = parse("<a>&lol9; &xxe;</a>", Limits::DEFAULT).unwrap();
        assert_eq!(root.text, "&lol9; &xxe;");
    }

    #[test]
    fn the_limits_are_hard() {
        let limits = Limits {
            max_bytes: 64,
            max_depth: 3,
            max_attributes: 2,
        };
        assert!(parse("<a><b><c/></b></a>", limits).is_ok());
        assert_eq!(
            parse("<a><b><c><d/></c></b></a>", limits),
            Err(XmlError::TooDeep)
        );
        assert!(parse("<a x='1' y='2'/>", limits).is_ok());
        assert_eq!(
            parse("<a x='1' y='2' z='3'/>", limits),
            Err(XmlError::TooManyAttributes)
        );
        // Namespace declarations count.
        assert_eq!(
            parse("<a xmlns='u' xmlns:b='v' z='3'/>", limits),
            Err(XmlError::TooManyAttributes)
        );
        let big = format!("<a>{}</a>", "x".repeat(64));
        assert_eq!(parse(&big, limits), Err(XmlError::TooLarge));
        // Depth is bounded however large the size limit is.
        let deep = "<a>".repeat(10_000);
        let roomy = Limits {
            max_bytes: 1 << 20,
            ..Limits::DEFAULT
        };
        assert_eq!(parse(&deep, roomy), Err(XmlError::TooDeep));
    }

    #[test]
    fn malformed_documents_are_errors_not_guesses() {
        let cases = [
            "",
            "   ",
            "text",
            "<a>",
            "<a></b>",
            "<a><b></a></b>",
            "</a>",
            "<a x></a>",
            "<a x=1></a>",
            "<a x=\"1></a>",
            "<a",
            "<>",
            "< a/>",
            "<a><!-- open</a>",
            "<a><![CDATA[open</a>",
            "<a><?open</a>",
            "<![CDATA[x]]><a/>",
            "<a =\"1\"/>",
        ];
        for src in cases {
            assert!(
                matches!(parse(src, Limits::DEFAULT), Err(XmlError::Malformed(_))),
                "{src:?} gave {:?}",
                parse(src, Limits::DEFAULT)
            );
        }
        assert!(XmlError::Malformed("x").to_string().contains("x"));
        assert!(!XmlError::Doctype.to_string().is_empty());
    }

    #[test]
    fn odd_but_legal_spellings_are_read() {
        let root = parse("<a\n  x = '1'\n/>", Limits::DEFAULT).unwrap();
        assert_eq!(root.attr("x"), Some("1"));
        let root = parse("<a ></a >", Limits::DEFAULT).unwrap();
        assert_eq!(root.local, "a");
        let root = parse("<p:a xmlns:p=\"u\" p:x=\"1\"/>", Limits::DEFAULT).unwrap();
        assert_eq!(root.attributes[0].prefix, "p");
        assert_eq!(root.attr("x"), Some("1"));
        // An empty namespace declaration undeclares the default namespace.
        let root = parse("<a xmlns=\"u\"><b xmlns=\"\"/></a>", Limits::DEFAULT).unwrap();
        assert_eq!(root.children[0].namespace, None);
    }
}
