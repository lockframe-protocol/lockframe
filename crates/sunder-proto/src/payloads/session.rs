//! Session management payload types.
//!
//! These payloads handle connection lifecycle: handshake, keepalive, and
//! disconnection.

use serde::{Deserialize, Serialize};

/// Initial client handshake
///
/// The first message sent by a client to establish a session. The server
/// responds with [`HelloReply`] containing a session ID.
///
/// # Security
///
/// - **Debug Redaction**: The `Debug` impl redacts `auth_token` to prevent
///   accidental logging of credentials. Always use custom `Debug`
///   implementations for types containing secrets.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Protocol version
    pub version: u8,
    /// Client capabilities (future use)
    pub capabilities: Vec<String>,
    /// Authentication token (optional)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth_token: Option<Vec<u8>>,
}

impl std::fmt::Debug for Hello {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hello")
            .field("version", &self.version)
            .field("capabilities", &self.capabilities)
            .field(
                "auth_token",
                &self.auth_token.as_ref().map(|token| format!("<redacted {} bytes>", token.len())),
            )
            .finish()
    }
}

/// Server response to Hello
///
/// Sent by the server after receiving [`Hello`]. Contains the assigned session
/// ID and optionally an authentication challenge.
///
/// # Security
///
/// - **Debug Redaction**: The `Debug` impl redacts `challenge` to prevent
///   logging cryptographic nonces or auth challenges.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloReply {
    /// Assigned session ID
    pub session_id: u64,
    /// Server capabilities
    pub capabilities: Vec<String>,
    /// Authentication challenge (if needed)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub challenge: Option<Vec<u8>>,
}

impl std::fmt::Debug for HelloReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelloReply")
            .field("session_id", &self.session_id)
            .field("capabilities", &self.capabilities)
            .field(
                "challenge",
                &self.challenge.as_ref().map(|ch| format!("<redacted {} bytes>", ch.len())),
            )
            .finish()
    }
}

/// Graceful disconnect
///
/// Sent by either client or server to terminate a session cleanly.
/// After sending or receiving `Goodbye`, both parties should close the
/// connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goodbye {
    /// Reason for disconnect (for logging/debugging)
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_serde() {
        let hello = Hello { version: 1, capabilities: vec!["mls".to_string()], auth_token: None };

        let cbor = ciborium::ser::into_writer(&hello, Vec::new());
        assert!(cbor.is_ok());
    }
}
