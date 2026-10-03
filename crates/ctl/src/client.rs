//! The control API's client: one HTTP request on one connection.
//!
//! Hand-written over `std::net::TcpStream`, as the server's side is. The
//! server answers every request `Connection: close` with a `Content-Length`,
//! so a request is: connect, write, read to the end, check the length. A
//! client that is not a browser sends no `Origin`, which is what the server's
//! same-origin rule for `POST` expects of one.
//!
//! What came back is one of three things, and they are the exit codes:
//! the state (the command applied, or the state was read), a catalog `error`
//! or `refused` message (the server refused), or anything else (not a chorus
//! server, or no answer at all).

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use chorus_control::json::{self, Value};

/// The most a response may be. A state message is a few kilobytes per room;
/// this is far above any real one and stops a peer that never stops sending.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Why a request did not end in a state message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// No usable answer: connect, timeout, or an answer that is not the
    /// control API's.
    Unreachable(String),
    /// The server's own `error` or `refused` message.
    Refused {
        /// The HTTP status code it came with.
        status: u16,
        /// The field the server names, or empty.
        field: String,
        /// The server's words.
        detail: String,
        /// The message, as received.
        body: String,
    },
}

/// The bytes of a `GET`.
pub fn get_request(server: &str, path: &str) -> String {
    format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        path, server
    )
}

/// The bytes of a `POST` of one catalog message.
pub fn post_request(server: &str, path: &str, body: &str) -> String {
    format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        path,
        server,
        body.len(),
        body
    )
}

/// Send `request` to `server` and read everything it answers, all within
/// `timeout` (counted on the monotonic clock).
fn exchange(server: &str, request: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let deadline = Instant::now() + timeout;
    let left = |what: &str| {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| format!("{} did not {} within {} s", server, what, timeout.as_secs()))
    };
    let targets = server
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {}: {}", server, e))?;
    let mut stream = None;
    let mut last = format!("{} resolves to no address", server);
    for target in targets {
        match TcpStream::connect_timeout(&target, left("accept a connection")?) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(e) => last = format!("cannot connect to {}: {}", server, e),
        }
    }
    let mut stream = stream.ok_or(last)?;
    stream
        .set_write_timeout(Some(left("take the request")?))
        .map_err(|e| format!("cannot set a timeout: {}", e))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("cannot send the request to {}: {}", server, e))?;
    let mut answer = Vec::new();
    let mut scratch = [0u8; 16 * 1024];
    loop {
        stream
            .set_read_timeout(Some(left("answer")?))
            .map_err(|e| format!("cannot set a timeout: {}", e))?;
        match stream.read(&mut scratch) {
            Ok(0) => return Ok(answer),
            Ok(n) => answer.extend_from_slice(&scratch[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(format!(
                    "{} did not answer within {} s",
                    server,
                    timeout.as_secs()
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("the answer from {} broke off: {}", server, e)),
        }
        if answer.len() > MAX_RESPONSE_BYTES {
            return Err(format!(
                "{} answered more than {} bytes",
                server, MAX_RESPONSE_BYTES
            ));
        }
    }
}

/// Split an HTTP/1.x response into its status code and its body, holding the
/// body to the `Content-Length` it declared.
pub fn read_response(raw: &[u8]) -> Result<(u16, String), String> {
    if raw.is_empty() {
        return Err("the connection closed without an answer".to_string());
    }
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("the answer is not an HTTP response (no end of headers)")?;
    let head = std::str::from_utf8(&raw[..split])
        .map_err(|_| "the answer's headers are not text".to_string())?;
    let body = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let mut parts = status_line.split(' ');
    let status = match (parts.next(), parts.next().map(str::parse::<u16>)) {
        (Some(version), Some(Ok(code)))
            if version.starts_with("HTTP/1.") && (100..=599).contains(&code) =>
        {
            code
        }
        _ => {
            return Err(format!(
                "the answer does not start with an HTTP status line: '{}'",
                status_line.chars().take(60).collect::<String>()
            ))
        }
    };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let declared = value
                .trim()
                .parse::<usize>()
                .map_err(|_| format!("the answer's Content-Length is '{}'", value.trim()))?;
            if declared != body.len() {
                return Err(format!(
                    "the answer was cut short: {} of {} bytes",
                    body.len(),
                    declared
                ));
            }
        }
    }
    let body = std::str::from_utf8(body)
        .map_err(|_| "the answer's body is not UTF-8".to_string())?
        .to_string();
    Ok((status, body))
}

/// Say what an answer is: the state (`Ok`, with it parsed), the server's
/// refusal, or not the control API's answer at all.
pub fn classify(status: u16, body: &str) -> Result<(String, Value), Failure> {
    let not_chorus = || {
        Failure::Unreachable(format!(
            "answered HTTP {} with a body that is not a control catalog message; is this the \
             server's --control-listen address?",
            status
        ))
    };
    let text = body.trim();
    let value = json::parse(text).map_err(|_| not_chorus())?;
    let words = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    match value.get("t").and_then(Value::as_str) {
        Some("state") if status == 200 => Ok((text.to_string(), value)),
        Some("error") | Some("refused") => Err(Failure::Refused {
            status,
            field: words("field"),
            detail: words("detail"),
            body: text.to_string(),
        }),
        _ => Err(not_chorus()),
    }
}

fn request(server: &str, request: &str, timeout: Duration) -> Result<(String, Value), Failure> {
    let raw = exchange(server, request, timeout).map_err(Failure::Unreachable)?;
    let (status, body) =
        read_response(&raw).map_err(|e| Failure::Unreachable(format!("{}: {}", server, e)))?;
    classify(status, &body).map_err(|failure| match failure {
        Failure::Unreachable(detail) => Failure::Unreachable(format!("{} {}", server, detail)),
        refused => refused,
    })
}

/// `GET /api/state`: the state message, as received and parsed.
pub fn state(server: &str, timeout: Duration) -> Result<(String, Value), Failure> {
    request(server, &get_request(server, "/api/state"), timeout)
}

/// `POST /api/command` with one catalog message: the state it answers.
pub fn command(server: &str, body: &str, timeout: Duration) -> Result<(String, Value), Failure> {
    request(server, &post_request(server, "/api/command", body), timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_carries_the_content_type_and_length_and_no_origin() {
        let body = r#"{"v":2,"t":"firmware_rescan"}"#;
        let request = post_request("127.0.0.1:8080", "/api/command", body);
        assert!(request.starts_with("POST /api/command HTTP/1.1\r\nHost: 127.0.0.1:8080\r\n"));
        assert!(request.contains("\r\nContent-Type: application/json\r\n"));
        assert!(request.contains(&format!("\r\nContent-Length: {}\r\n", body.len())));
        assert!(request.ends_with(&format!("\r\n\r\n{}", body)));
        assert!(!request.to_ascii_lowercase().contains("origin"));
    }

    #[test]
    fn a_response_is_held_to_its_status_line_and_its_length() {
        let good = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}";
        assert_eq!(read_response(good), Ok((200, "{}".to_string())));
        let short = b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n{}";
        assert!(read_response(short).unwrap_err().contains("cut short"));
        assert!(read_response(b"")
            .unwrap_err()
            .contains("without an answer"));
        assert!(read_response(b"SSH-2.0-x\r\n\r\n")
            .unwrap_err()
            .contains("status line"));
        assert!(read_response(b"HTTP/1.1 200 OK\r\n")
            .unwrap_err()
            .contains("not an HTTP"));
    }

    #[test]
    fn an_answer_is_the_state_a_refusal_or_not_the_control_api() {
        let state = r#"{"v":2,"t":"state","serial":0,"zones":[]}"#;
        assert!(classify(200, state).is_ok());
        let error = r#"{"v":1,"t":"error","field":"zone","detail":"no"}"#;
        assert_eq!(
            classify(400, error),
            Err(Failure::Refused {
                status: 400,
                field: "zone".to_string(),
                detail: "no".to_string(),
                body: error.to_string(),
            })
        );
        for (status, body) in [
            (200, "<html>"),
            (200, "{}"),
            (502, "Bad Gateway"),
            (404, state),
        ] {
            assert!(
                matches!(classify(status, body), Err(Failure::Unreachable(_))),
                "{} {}",
                status,
                body
            );
        }
    }
}
