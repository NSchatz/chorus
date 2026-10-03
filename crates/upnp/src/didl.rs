//! DIDL-Lite: the metadata a control point sends with a URI.
//!
//! `SetAVTransportURI` and `SetNextAVTransportURI` carry "a DIDL-Lite XML
//! fragment (defined in the ContentDirectory service template)" describing
//! the resource, or an empty string when the control point has nothing to
//! say (AVT1 section 2.4.1). chorus wants only what it shows (title, artist,
//! album, cover art address) and a duration hint, so the reading is
//! tolerant to the point of never failing: empty, `NOT_IMPLEMENTED`,
//! garbage, an oversize document, or one bearing a DOCTYPE all mean "no
//! metadata" ([`parse`] returns `None`), and the action carries on.
//!
//! The metadata string itself is never rewritten: the state machine keeps
//! what was sent, byte for byte, and returns exactly that from GetMediaInfo,
//! GetPositionInfo and LastChange, because control points compare it with
//! what they sent.
//!
//! The album art address is kept as text only. Nothing in chorus fetches it;
//! whatever displays it does.

use crate::time;
use crate::xml::{self, Limits, Node};

/// The DIDL-Lite namespace (CDS1 section 2.8, the examples' root element).
pub const DIDL_NS: &str = "urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/";
/// The Dublin Core namespace DIDL-Lite uses for `dc:title` and `dc:creator`.
pub const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
/// The UPnP metadata namespace of `upnp:artist`, `upnp:album`,
/// `upnp:albumArtURI` and `upnp:class`.
pub const UPNP_NS: &str = "urn:schemas-upnp-org:metadata-1-0/upnp/";

/// The longest display string kept, in characters; longer ones are cut.
pub const MAX_TEXT: usize = 1024;
/// The longest address kept, in bytes; a longer one is dropped, since a cut
/// address is a wrong address.
pub const MAX_URL: usize = 2048;

/// A `protocolInfo` value split into its four fields:
/// `<protocol>:<network>:<contentFormat>:<additionalInfo>` (CM1 section
/// 2.5.2). For `http-get` the network is `*` and the content format is the
/// MIME type. The fourth field is where control points put `DLNA.ORG_PN`
/// and its relatives; it is kept as opaque text and never interpreted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolInfo {
    /// For example `http-get`.
    pub protocol: String,
    /// For example `*`.
    pub network: String,
    /// For example `audio/flac`, possibly with parameters
    /// (`audio/L16;rate=44100;channels=2`).
    pub content_format: String,
    /// Everything after the third colon, as sent.
    pub additional_info: String,
}

impl ProtocolInfo {
    /// Splits a `protocolInfo` string; `None` unless it has all four fields.
    pub fn parse(text: &str) -> Option<ProtocolInfo> {
        let mut parts = text.trim().splitn(4, ':');
        Some(ProtocolInfo {
            protocol: parts.next()?.to_string(),
            network: parts.next()?.to_string(),
            content_format: parts.next()?.to_string(),
            additional_info: parts.next()?.to_string(),
        })
    }

    /// The MIME type alone: the content format before any `;`, lower case.
    pub fn mime(&self) -> String {
        self.content_format
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
    }
}

/// One `res` element: an address the item can be fetched from, and what the
/// control point says about it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Res {
    /// The address (the element's text).
    pub uri: String,
    /// `res@protocolInfo`, split.
    pub protocol_info: Option<ProtocolInfo>,
    /// `res@duration`, in milliseconds.
    pub duration_ms: Option<u64>,
    /// `res@sampleFrequency`, in hertz.
    pub sample_frequency: Option<u32>,
    /// `res@nrAudioChannels`.
    pub nr_audio_channels: Option<u32>,
    /// `res@size`, in bytes.
    pub size: Option<u64>,
}

/// What chorus takes from a DIDL-Lite document: its first item.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Metadata {
    /// `dc:title`.
    pub title: Option<String>,
    /// The first `upnp:artist` (whatever its `role`), else `dc:creator`.
    pub artist: Option<String>,
    /// `upnp:album`.
    pub album: Option<String>,
    /// `upnp:albumArtURI`, as text.
    pub album_art_uri: Option<String>,
    /// `upnp:class`, for example `object.item.audioItem.musicTrack`.
    pub class: Option<String>,
    /// Every `res` in document order.
    pub resources: Vec<Res>,
}

impl Metadata {
    /// The `res` that describes `uri`: the one whose address equals it, else
    /// the first.
    pub fn resource_for(&self, uri: &str) -> Option<&Res> {
        self.resources
            .iter()
            .find(|r| r.uri == uri)
            .or_else(|| self.resources.first())
    }

    /// The duration the control point claims for `uri`, if any.
    pub fn duration_for(&self, uri: &str) -> Option<u64> {
        self.resource_for(uri).and_then(|r| r.duration_ms)
    }
}

fn text_of(item: &Node, namespace: &str, local: &str) -> Option<String> {
    // By namespace first; by local name alone when a control point got its
    // namespaces wrong or left them out.
    let node = item
        .child_ns(namespace, local)
        .or_else(|| item.child(local))?;
    let text = node.text.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(MAX_TEXT).collect())
}

fn url_of(item: &Node, namespace: &str, local: &str) -> Option<String> {
    let node = item
        .child_ns(namespace, local)
        .or_else(|| item.child(local))?;
    let text = node.text.trim();
    (!text.is_empty() && text.len() <= MAX_URL).then(|| text.to_string())
}

fn number<T: std::str::FromStr>(node: &Node, attr: &str) -> Option<T> {
    let v = node.attr(attr)?.trim();
    if v.is_empty() || v.len() > 19 || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse().ok()
}

/// Reads a metadata argument. `None` means "no metadata", for every reason
/// there can be: an empty or white-space string, `NOT_IMPLEMENTED`, text
/// that is not XML, XML over the reader's limits, a DOCTYPE or entity
/// declaration, or a document with no `item`. Never an error: metadata is a
/// courtesy, and an action must not fail over it.
///
/// Read with tolerance for what control points do: the document inside a
/// CDATA section, the document escaped once more than it should be, any
/// namespace prefixes, elements matched by local name when the namespaces
/// are missing, extra elements and attributes of any namespace.
pub fn parse(text: &str) -> Option<Metadata> {
    let mut text = text.trim();
    if text.is_empty() || text == time::NOT_IMPLEMENTED {
        return None;
    }
    if let Some(inner) = text
        .strip_prefix("<![CDATA[")
        .and_then(|t| t.strip_suffix("]]>"))
    {
        text = inner.trim();
    }
    let unescaped;
    if text.starts_with("&lt;") {
        unescaped = xml::unescape(text);
        text = unescaped.trim();
    }
    let root = xml::parse(text, Limits::DEFAULT).ok()?;
    let item = root.find("item")?;
    let resources = item
        .children_named("res")
        .filter_map(|res| {
            let uri = res.text.trim();
            if uri.is_empty() || uri.len() > MAX_URL {
                return None;
            }
            Some(Res {
                uri: uri.to_string(),
                protocol_info: res.attr("protocolInfo").and_then(ProtocolInfo::parse),
                duration_ms: res.attr("duration").and_then(time::parse),
                sample_frequency: number(res, "sampleFrequency"),
                nr_audio_channels: number(res, "nrAudioChannels"),
                size: number(res, "size"),
            })
        })
        .collect();
    Some(Metadata {
        title: text_of(item, DC_NS, "title"),
        artist: text_of(item, UPNP_NS, "artist").or_else(|| text_of(item, DC_NS, "creator")),
        album: text_of(item, UPNP_NS, "album"),
        album_art_uri: url_of(item, UPNP_NS, "albumArtURI"),
        class: text_of(item, UPNP_NS, "class"),
        resources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">
  <item id="1" parentID="0" restricted="1">
    <dc:title>A &amp; B &lt;"x"&gt; é</dc:title>
    <dc:creator>The Creator</dc:creator>
    <upnp:artist role="Performer">The Artist</upnp:artist>
    <upnp:artist role="Composer">Someone Else</upnp:artist>
    <upnp:album>The Album</upnp:album>
    <upnp:albumArtURI>http://192.0.2.50:57645/art.jpg?a=1&amp;b=2</upnp:albumArtURI>
    <upnp:class>object.item.audioItem.musicTrack</upnp:class>
    <res protocolInfo="http-get:*:audio/flac:*" duration="0:03:25.000" size="23456789" sampleFrequency="44100" nrAudioChannels="2" bitsPerSample="16">http://192.0.2.50:57645/track.flac</res>
    <res protocolInfo="http-get:*:audio/mpeg:DLNA.ORG_PN=MP3;DLNA.ORG_OP=01" duration="0:03:26">http://192.0.2.50:57645/track.mp3</res>
  </item>
</DIDL-Lite>"#;

    #[test]
    fn a_full_item_is_read() {
        let m = parse(FULL).unwrap();
        assert_eq!(m.title.as_deref(), Some("A & B <\"x\"> \u{e9}"));
        assert_eq!(m.artist.as_deref(), Some("The Artist"));
        assert_eq!(m.album.as_deref(), Some("The Album"));
        assert_eq!(
            m.album_art_uri.as_deref(),
            Some("http://192.0.2.50:57645/art.jpg?a=1&b=2")
        );
        assert_eq!(m.class.as_deref(), Some("object.item.audioItem.musicTrack"));
        assert_eq!(m.resources.len(), 2);
        let flac = &m.resources[0];
        assert_eq!(flac.uri, "http://192.0.2.50:57645/track.flac");
        assert_eq!(flac.duration_ms, Some(205_000));
        assert_eq!(flac.size, Some(23_456_789));
        assert_eq!(flac.sample_frequency, Some(44_100));
        assert_eq!(flac.nr_audio_channels, Some(2));
        let pi = flac.protocol_info.as_ref().unwrap();
        assert_eq!(
            (
                pi.protocol.as_str(),
                pi.network.as_str(),
                pi.content_format.as_str(),
                pi.additional_info.as_str()
            ),
            ("http-get", "*", "audio/flac", "*")
        );
        // The DLNA fields are carried as opaque text.
        let mp3 = m.resources[1].protocol_info.as_ref().unwrap();
        assert_eq!(mp3.additional_info, "DLNA.ORG_PN=MP3;DLNA.ORG_OP=01");
        assert_eq!(mp3.mime(), "audio/mpeg");
        // The res for a URI, else the first.
        assert_eq!(
            m.duration_for("http://192.0.2.50:57645/track.mp3"),
            Some(206_000)
        );
        assert_eq!(m.duration_for("http://192.0.2.50/other"), Some(205_000));
    }

    #[test]
    fn creator_stands_in_for_a_missing_artist() {
        let m = parse(
            "<DIDL-Lite><item><dc:title>T</dc:title><dc:creator>C</dc:creator></item></DIDL-Lite>",
        )
        .unwrap();
        assert_eq!(m.artist.as_deref(), Some("C"));
        assert_eq!(
            m.title.as_deref(),
            Some("T"),
            "matched by local name, no namespaces declared"
        );
        assert!(m.resources.is_empty());
        assert_eq!(m.resource_for("x"), None);
        assert_eq!(m.duration_for("x"), None);
    }

    #[test]
    fn what_is_not_metadata_is_no_metadata() {
        let big = format!(
            "<DIDL-Lite><item><dc:title>{}</dc:title></item></DIDL-Lite>",
            "x".repeat(70_000)
        );
        let hostile = "<?xml version=\"1.0\"?><!DOCTYPE DIDL-Lite [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;\"><!ENTITY x SYSTEM \"file:///etc/passwd\">]><DIDL-Lite><item><dc:title>&b;&x;</dc:title></item></DIDL-Lite>";
        for none in [
            "",
            "   ",
            "NOT_IMPLEMENTED",
            "garbage",
            "<DIDL-Lite>",
            "<DIDL-Lite></DIDL-Lite>",
            "<DIDL-Lite><container id=\"1\"><dc:title>C</dc:title></container></DIDL-Lite>",
            "<a><b></a></b>",
            "\u{0}\u{1}\u{2}",
            hostile,
            big.as_str(),
        ] {
            assert_eq!(parse(none), None, "{}", &none[..none.len().min(40)]);
        }
    }

    #[test]
    fn cdata_wrapped_and_twice_escaped_documents_are_read() {
        let doc = "<DIDL-Lite><item><dc:title>T &amp; U</dc:title></item></DIDL-Lite>";
        let cdata = format!("<![CDATA[{doc}]]>");
        assert_eq!(parse(&cdata).unwrap().title.as_deref(), Some("T & U"));
        let escaped = crate::xml::escape_text(doc);
        assert_eq!(parse(&escaped).unwrap().title.as_deref(), Some("T & U"));
    }

    #[test]
    fn strings_are_bounded() {
        let long = "y".repeat(3000);
        let doc = format!(
            "<DIDL-Lite><item><dc:title>{long}</dc:title><upnp:albumArtURI>http://192.0.2.1/{long}</upnp:albumArtURI><res>http://192.0.2.1/{long}</res><res duration=\"x\" size=\"-1\" sampleFrequency=\"\" nrAudioChannels=\"two\" protocolInfo=\"bad\">http://192.0.2.1/ok</res><res></res></item></DIDL-Lite>"
        );
        let m = parse(&doc).unwrap();
        assert_eq!(m.title.unwrap().chars().count(), MAX_TEXT);
        assert_eq!(
            m.album_art_uri, None,
            "an over-long address is dropped, not cut"
        );
        assert_eq!(m.resources.len(), 1);
        assert_eq!(
            m.resources[0],
            Res {
                uri: "http://192.0.2.1/ok".into(),
                ..Res::default()
            }
        );
    }

    #[test]
    fn protocol_info_needs_four_fields() {
        assert_eq!(ProtocolInfo::parse("http-get:*:audio/flac"), None);
        assert_eq!(ProtocolInfo::parse(""), None);
        let l16 =
            ProtocolInfo::parse("http-get:*:audio/L16;rate=44100;channels=2:DLNA.ORG_PN=LPCM;x:y")
                .unwrap();
        assert_eq!(l16.mime(), "audio/l16");
        assert_eq!(l16.additional_info, "DLNA.ORG_PN=LPCM;x:y");
    }
}
