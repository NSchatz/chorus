//! The supervisor's log: lines on stderr, with the API key taken out of
//! every one of them.
//!
//! Soloist accepts its API key only as a command-line argument, and may
//! repeat it in its own output. The key therefore passes through this
//! process, and everything this process writes (its own lines, Soloist's
//! output passed through, the events relayed to the server) goes through
//! [`redact`] with the key's value first.

use std::sync::{Arc, Mutex};

/// What a redacted secret is replaced with.
pub const REDACTED: &str = "[redacted]";

/// `text` with every occurrence of `secret` replaced by [`REDACTED`]. An
/// empty secret redacts nothing.
pub fn redact(text: &str, secret: &str) -> String {
    if secret.is_empty() || !text.contains(secret) {
        return text.to_string();
    }
    text.replace(secret, REDACTED)
}

/// A handle on the log, shared by every thread.
#[derive(Clone, Default)]
pub struct Log {
    secret: Arc<Mutex<String>>,
}

impl Log {
    /// A log with no secret yet.
    pub fn new() -> Log {
        Log::default()
    }

    /// The secret to redact from now on (the API key as last read).
    pub fn set_secret(&self, secret: &str) {
        *self.lock() = secret.to_string();
    }

    /// `text` with the secret taken out.
    pub fn clean(&self, text: &str) -> String {
        redact(text, &self.lock())
    }

    /// Write one line.
    pub fn line(&self, text: &str) {
        eprintln!("chorus-soloistd: {}", self.clean(text));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, String> {
        // A poisoned lock still holds the secret; redacting matters more
        // than the panic that poisoned it.
        self.secret.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secret_is_replaced_wherever_it_stands() {
        let key = "the-key-value+/=";
        assert_eq!(
            redact(
                &format!("soloist -n Kitchen -k {key} --ws 127.0.0.1:0"),
                key
            ),
            "soloist -n Kitchen -k [redacted] --ws 127.0.0.1:0"
        );
        assert_eq!(
            redact(&format!("{key}{key} x{key}y"), key),
            "[redacted][redacted] x[redacted]y"
        );
        assert_eq!(
            redact(&format!(r#"{{"api_key":"{key}"}}"#), key),
            r#"{"api_key":"[redacted]"}"#
        );
        assert_eq!(redact("nothing here", key), "nothing here");
    }

    #[test]
    fn an_empty_secret_redacts_nothing() {
        assert_eq!(redact("a b c", ""), "a b c");
    }

    #[test]
    fn the_log_redacts_with_the_secret_it_was_last_given() {
        let log = Log::new();
        assert_eq!(log.clean("key one"), "key one");
        log.set_secret("one");
        assert_eq!(log.clone().clean("key one"), "key [redacted]");
        log.set_secret("two");
        assert_eq!(log.clean("one two"), "one [redacted]");
    }
}
