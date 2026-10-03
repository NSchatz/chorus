//! ConnectionManager:1: which formats the renderer takes.
//!
//! CM1. A renderer has nothing to offer as a source, so `Source` is empty,
//! and `Sink` lists what it can be handed. With no PrepareForConnection
//! there is exactly one connection, ID 0 (CM1 section 2.2.3: the variable
//! "should be set to '0'").

use crate::soap::Invocation;
use crate::{error, Outputs, UpnpError};

/// The MIME types of the Sink list, in the order listed.
///
/// What chorus decodes: MP3 (`audio/mpeg`), FLAC, Ogg Vorbis and Ogg Opus
/// (both arrive as `audio/ogg` or `application/ogg`; the codec is read from
/// the stream), WAV, raw big-endian 16-bit PCM (`audio/L16`, with its rate
/// and channels as parameters), and ALAC in MP4. The second spellings
/// (`x-flac`, `x-wav`, `wave`, `x-m4a`, `m4a`) are there because media
/// servers disagree on names and a control point matches these strings
/// literally against what the server offers.
///
/// **AAC is not here**, under any name: not `audio/aac`, `audio/aacp`,
/// `audio/x-aac` or `audio/vnd.dlna.adts`. chorus has no AAC decoder.
///
/// **`audio/mp4` is here, and that needs saying plainly.** MP4 is a
/// container, and `audio/mp4` (with `audio/x-m4a` and `audio/m4a`) is the
/// only name ALAC has. The same name is what most AAC files carry. chorus
/// lists it so that ALAC can be played at all, and decides at play time from
/// the file itself: ALAC in the MP4 plays; AAC in the MP4 is refused, by
/// name, when the stream is opened (the player reports the failure and the
/// transport goes to STOPPED with TransportStatus ERROR_OCCURRED). So this
/// list promises a container, not every codec that can be inside it. The
/// alternative, leaving `audio/mp4` out, would make ALAC unplayable from any
/// server that does not transcode.
pub const SINK_MIME_TYPES: [&str; 12] = [
    "audio/mpeg",
    "audio/flac",
    "audio/x-flac",
    "audio/ogg",
    "application/ogg",
    "audio/wav",
    "audio/x-wav",
    "audio/wave",
    "audio/L16",
    "audio/mp4",
    "audio/x-m4a",
    "audio/m4a",
];

/// `SinkProtocolInfo`: one `http-get:*:<mime>:*` per type, joined by commas
/// with no spaces (CM1 section 2.5.2: `<protocol>:<network>:<contentFormat>:
/// <additionalInfo>`; for `http-get` the network is `*`). The fourth field is
/// always `*`: chorus names no DLNA media format profile (`DLNA.ORG_PN`),
/// which belong to guidelines it does not implement or claim.
pub fn sink_protocol_info() -> String {
    SINK_MIME_TYPES
        .iter()
        .map(|mime| format!("http-get:*:{mime}:*"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Whether a content type is on the Sink list: the type before any `;`
/// parameter, compared without regard to case (`audio/L16;rate=44100;
/// channels=2` is `audio/L16`).
pub fn sink_accepts(content_type: &str) -> bool {
    let mime = content_type.split(';').next().unwrap_or("").trim();
    SINK_MIME_TYPES.iter().any(|m| m.eq_ignore_ascii_case(mime))
}

/// The three directly evented variables with their values, for the initial
/// event of a subscription (CM1 section 2.3, table 2: all three evented,
/// none moderated). They never change afterwards.
pub fn evented() -> [(&'static str, String); 3] {
    [
        ("SourceProtocolInfo", String::new()),
        ("SinkProtocolInfo", sink_protocol_info()),
        ("CurrentConnectionIDs", "0".to_string()),
    ]
}

/// Performs a ConnectionManager action that passed
/// [`crate::soap::validate`].
///
/// - `GetProtocolInfo`: `Source` empty, `Sink` the list.
/// - `GetCurrentConnectionIDs`: `0`.
/// - `GetCurrentConnectionInfo` for connection 0 returns what CM1 section
///   2.4.5 asks of a device without PrepareForConnection: RcsID 0,
///   AVTransportID 0, ProtocolInfo empty ("NULL (empty string)" when not
///   known), PeerConnectionManager empty, PeerConnectionID -1, Direction
///   `Input`, Status `OK`. Any other ID is 706 Invalid connection reference;
///   text that is not an `i4` is 402.
pub fn invoke(invocation: &Invocation) -> Result<Outputs, UpnpError> {
    match invocation.action.name {
        "GetProtocolInfo" => Ok(vec![
            ("Source", String::new()),
            ("Sink", sink_protocol_info()),
        ]),
        "GetCurrentConnectionIDs" => Ok(vec![("ConnectionIDs", "0".to_string())]),
        "GetCurrentConnectionInfo" => {
            let id = invocation
                .input("ConnectionID")
                .trim()
                .parse::<i32>()
                .map_err(|_| error::INVALID_ARGS)?;
            if id != 0 {
                return Err(error::CM_INVALID_CONNECTION_REFERENCE);
            }
            Ok(vec![
                ("RcsID", "0".to_string()),
                ("AVTransportID", "0".to_string()),
                ("ProtocolInfo", String::new()),
                ("PeerConnectionManager", String::new()),
                ("PeerConnectionID", "-1".to_string()),
                ("Direction", "Input".to_string()),
                ("Status", "OK".to_string()),
            ])
        }
        _ => Err(error::INVALID_ACTION),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::description::CONNECTION_MANAGER;
    use crate::soap::{validate, ActionRequest};
    use crate::Service;

    fn call(action: &str, args: &[(&str, &str)]) -> Result<Outputs, UpnpError> {
        let request = ActionRequest {
            service_type: Service::ConnectionManager.service_type().into(),
            action: action.into(),
            arguments: args
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
        };
        invoke(&validate(Service::ConnectionManager, &request)?)
    }

    #[test]
    fn the_sink_list_is_exactly_this() {
        assert_eq!(
            sink_protocol_info(),
            "http-get:*:audio/mpeg:*,http-get:*:audio/flac:*,http-get:*:audio/x-flac:*,http-get:*:audio/ogg:*,http-get:*:application/ogg:*,http-get:*:audio/wav:*,http-get:*:audio/x-wav:*,http-get:*:audio/wave:*,http-get:*:audio/L16:*,http-get:*:audio/mp4:*,http-get:*:audio/x-m4a:*,http-get:*:audio/m4a:*"
        );
    }

    #[test]
    fn aac_and_dlna_profiles_are_never_listed() {
        let sink = sink_protocol_info();
        let lower = sink.to_ascii_lowercase();
        for never in ["aac", "adts", "dlna", "3gpp", " "] {
            assert!(!lower.contains(never), "{never}");
        }
        for entry in sink.split(',') {
            let fields: Vec<&str> = entry.split(':').collect();
            assert_eq!(fields.len(), 4, "{entry}");
            assert_eq!((fields[0], fields[1], fields[3]), ("http-get", "*", "*"));
        }
        for refused in [
            "audio/aac",
            "audio/aacp",
            "audio/x-aac",
            "audio/vnd.dlna.adts",
            "video/mp4",
            "",
        ] {
            assert!(!sink_accepts(refused), "{refused}");
        }
        for taken in [
            "audio/mpeg",
            "AUDIO/FLAC",
            "audio/L16;rate=44100;channels=2",
            "audio/l16",
            "audio/mp4",
            " application/ogg ",
        ] {
            assert!(sink_accepts(taken), "{taken}");
        }
    }

    #[test]
    fn the_actions_answer_in_scpd_order() {
        let out = call("GetProtocolInfo", &[]).unwrap();
        assert_eq!(out[0], ("Source", String::new()));
        assert_eq!(out[1], ("Sink", sink_protocol_info()));
        assert_eq!(
            call("GetCurrentConnectionIDs", &[]).unwrap(),
            [("ConnectionIDs", "0".to_string())]
        );
        let info = call("GetCurrentConnectionInfo", &[("ConnectionID", "0")]).unwrap();
        let values: Vec<&str> = info.iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(values, ["0", "0", "", "", "-1", "Input", "OK"]);
        // Each answer names the table's out arguments, in its order.
        for (action, args) in [
            ("GetProtocolInfo", &[][..]),
            ("GetCurrentConnectionIDs", &[][..]),
            ("GetCurrentConnectionInfo", &[("ConnectionID", "0")][..]),
        ] {
            let names: Vec<&str> = call(action, args)
                .unwrap()
                .iter()
                .map(|(n, _)| *n)
                .collect();
            let table: Vec<&str> = CONNECTION_MANAGER
                .action(action)
                .unwrap()
                .outputs()
                .map(|a| a.name)
                .collect();
            assert_eq!(names, table, "{action}");
        }
    }

    #[test]
    fn another_connection_is_706() {
        for id in ["1", "-1", "7"] {
            assert_eq!(
                call("GetCurrentConnectionInfo", &[("ConnectionID", id)]),
                Err(error::CM_INVALID_CONNECTION_REFERENCE)
            );
        }
        assert_eq!(
            call("GetCurrentConnectionInfo", &[("ConnectionID", "zero")]),
            Err(error::INVALID_ARGS)
        );
        assert_eq!(
            call("GetCurrentConnectionInfo", &[]),
            Err(error::INVALID_ARGS)
        );
        assert_eq!(
            call("PrepareForConnection", &[]),
            Err(error::INVALID_ACTION)
        );
    }

    #[test]
    fn the_initial_event_has_the_three_variables() {
        let e = evented();
        assert_eq!(e[0], ("SourceProtocolInfo", String::new()));
        assert_eq!(e[1].1, sink_protocol_info());
        assert_eq!(e[2], ("CurrentConnectionIDs", "0".to_string()));
        let evented_in_table: Vec<&str> = CONNECTION_MANAGER
            .variables
            .iter()
            .filter(|v| v.evented)
            .map(|v| v.name)
            .collect();
        assert_eq!(
            e.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            evented_in_table
        );
    }
}
