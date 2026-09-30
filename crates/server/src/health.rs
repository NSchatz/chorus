//! `chorus-server --health-check <addr:port>`: ask a running server's control
//! plane whether it answers, and exit 0 if it does.
//!
//! This exists for container healthchecks. The released image is distroless
//! (no shell, no curl), so a compose `healthcheck:` has nothing to run except
//! the server binary itself; the homelab's baseline requires a healthcheck on
//! every service. The probe starts no thread, binds nothing and touches no
//! audio: it opens one TCP connection to the control plane, sends
//! `GET /api/state`, and reads the status line.
//!
//! Exit codes follow Docker's healthcheck contract: "0: success - the container
//! is healthy and ready for use", "1: unhealthy - the container isn't working
//! correctly", "2: reserved - don't use this exit code" (Dockerfile reference,
//! HEALTHCHECK, https://docs.docker.com/reference/dockerfile/#healthcheck; its
//! source https://github.com/moby/buildkit/blob/master/frontend/dockerfile/docs/reference.md,
//! read 2026-09-30).

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// How long the probe waits for a connection and for the status line. A
/// healthcheck's own `timeout:` bounds the whole run; this keeps one probe
/// from outliving it.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Probe the control plane at `address` (`host:port`). `Ok` carries the status
/// line of a 200 answer; `Err` says what failed, in words a `docker inspect`
/// reader can act on.
pub fn probe(address: &str, timeout: Duration) -> Result<String, String> {
    let targets: Vec<SocketAddr> = address
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {address}: {e}"))?
        .collect();
    let target = targets
        .first()
        .ok_or_else(|| format!("{address} resolves to no address"))?;
    let mut stream = TcpStream::connect_timeout(target, timeout)
        .map_err(|e| format!("cannot connect to {address}: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|()| stream.set_write_timeout(Some(timeout)))
        .map_err(|e| format!("cannot set timeouts: {e}"))?;
    write!(
        stream,
        "GET /api/state HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|e| format!("cannot send the request to {address}: {e}"))?;
    let mut status = String::new();
    BufReader::new(stream)
        .read_line(&mut status)
        .map_err(|e| format!("no answer from {address}: {e}"))?;
    let status = status.trim_end().to_string();
    let code = status.split_whitespace().nth(1);
    if status.starts_with("HTTP/1.") && code == Some("200") {
        Ok(status)
    } else if status.is_empty() {
        Err(format!(
            "{address} closed the connection without a status line"
        ))
    } else {
        Err(format!("{address} answered `{status}`, not 200"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::thread;

    /// A one-shot server that reads the request and answers `reply`.
    fn answer_once(reply: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = [0u8; 512];
            let n = conn.read(&mut buf).unwrap();
            assert!(buf[..n].starts_with(b"GET /api/state HTTP/1.1\r\n"));
            conn.write_all(reply.as_bytes()).unwrap();
        });
        address
    }

    #[test]
    fn a_200_is_healthy() {
        let address = answer_once("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        assert_eq!(probe(&address, PROBE_TIMEOUT).unwrap(), "HTTP/1.1 200 OK");
    }

    #[test]
    fn another_status_is_unhealthy_and_named() {
        let address = answer_once("HTTP/1.1 503 Service Unavailable\r\n\r\n");
        let e = probe(&address, PROBE_TIMEOUT).unwrap_err();
        assert!(e.contains("503"), "{e}");
    }

    #[test]
    fn a_silent_close_is_unhealthy() {
        let address = answer_once("");
        let e = probe(&address, PROBE_TIMEOUT).unwrap_err();
        assert!(e.contains("without a status line"), "{e}");
    }

    #[test]
    fn nothing_listening_is_unhealthy() {
        // Bind, learn the port, and close it again, so nothing listens there.
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .to_string();
        let e = probe(&address, PROBE_TIMEOUT).unwrap_err();
        assert!(e.contains("cannot connect"), "{e}");
    }
}
