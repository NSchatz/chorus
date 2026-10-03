//! TLS for https URLs: rustls with the ring provider, the roots read from a
//! PEM bundle on disk.
//!
//! The provider is handed to the configuration explicitly; nothing here reads
//! or installs a process-wide default. Certificate and host name verification
//! are rustls's own and there is no way to turn them off. (API as documented
//! at https://docs.rs/rustls/0.23.45/rustls/, read 2026-10-03.)
//!
//! Certificate validity is the one place a fetch depends on the wall clock:
//! rustls compares the certificate's dates with the system time. No code in
//! this crate reads it, and nothing on the audio path depends on it.

use std::io;
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::error::FetchError;
use crate::policy::Policy;
use crate::url::Url;

/// Where the roots are read from when neither the policy nor `SSL_CERT_FILE`
/// names a bundle: the path Debian's ca-certificates package writes, which the
/// release image's distroless base ships
/// (https://github.com/GoogleContainerTools/distroless/blob/main/base/README.md,
/// read 2026-10-03).
pub const DEFAULT_CA_BUNDLE: &str = "/etc/ssl/certs/ca-certificates.crt";

/// The bundle a policy reads its roots from: `Policy::ca_bundle`, else the
/// file `SSL_CERT_FILE` names, else [`DEFAULT_CA_BUNDLE`].
pub fn ca_bundle_path(policy: &Policy) -> PathBuf {
    if let Some(path) = &policy.ca_bundle {
        return path.clone();
    }
    match std::env::var_os("SSL_CERT_FILE") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(DEFAULT_CA_BUNDLE),
    }
}

/// The client configuration for a policy. A bundle that is missing,
/// unreadable or holds no usable certificate is `FetchError::Tls` naming the
/// path that was tried.
pub(crate) fn client_config(policy: &Policy) -> Result<Arc<ClientConfig>, FetchError> {
    let path = ca_bundle_path(policy);
    let shown = path.display();
    let certs = CertificateDer::pem_file_iter(&path)
        .map_err(|e| FetchError::Tls(format!("ca bundle {shown}: cannot be read: {e}")))?;
    let mut roots = RootCertStore::empty();
    let (added, _ignored) = roots.add_parsable_certificates(certs.filter_map(Result::ok));
    if added == 0 {
        return Err(FetchError::Tls(format!(
            "ca bundle {shown}: holds no usable certificate"
        )));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| FetchError::Tls(format!("protocol versions: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    // The client speaks HTTP/1.1 only; saying so keeps a server from choosing h2.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Runs the handshake to its end on a connected socket, so a certificate
/// that is not accepted is an error here and not at the first read.
pub(crate) fn handshake(
    config: Arc<ClientConfig>,
    url: &Url,
    sock: TcpStream,
) -> Result<StreamOwned<ClientConnection, TcpStream>, FetchError> {
    let host = &url.host;
    let name = ServerName::try_from(host.clone()).map_err(|_| {
        FetchError::Tls(format!(
            "{host}: not a name a certificate can be checked for"
        ))
    })?;
    let conn =
        ClientConnection::new(config, name).map_err(|e| FetchError::Tls(format!("{host}: {e}")))?;
    let mut tls = StreamOwned::new(conn, sock);
    while tls.conn.is_handshaking() {
        if let Err(e) = tls.conn.complete_io(&mut tls.sock) {
            return Err(handshake_error(host, e));
        }
    }
    Ok(tls)
}

fn handshake_error(host: &str, e: io::Error) -> FetchError {
    if let Some(tls) = e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    {
        return FetchError::Tls(format!("{host}: {tls}"));
    }
    match e.kind() {
        io::ErrorKind::UnexpectedEof => FetchError::Tls(format!(
            "{host}: the connection closed during the handshake"
        )),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => FetchError::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            "the tls handshake did not finish within the read timeout",
        )),
        _ => FetchError::Io(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_names_the_bundle_first() {
        let policy = Policy {
            ca_bundle: Some(PathBuf::from("/nonexistent/chorus-test/roots.pem")),
            ..Policy::default()
        };
        assert_eq!(
            ca_bundle_path(&policy),
            PathBuf::from("/nonexistent/chorus-test/roots.pem")
        );
    }

    #[test]
    fn a_missing_bundle_is_the_named_error() {
        let policy = Policy {
            ca_bundle: Some(PathBuf::from("/nonexistent/chorus-test/roots.pem")),
            ..Policy::default()
        };
        match client_config(&policy) {
            Err(FetchError::Tls(why)) => assert!(
                why.starts_with("ca bundle /nonexistent/chorus-test/roots.pem: cannot be read"),
                "{why}"
            ),
            other => panic!("{:?}", other.map(|_| ())),
        }
    }
}
