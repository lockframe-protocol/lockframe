//! Application message payload types.
//!
//! These payloads handle user-visible messages: encrypted content, delivery
//! receipts, and reactions.

use serde::{Deserialize, Serialize};

/// Encrypted application message
///
/// This is the primary message type for user-to-user communication.
/// The message is encrypted with sender keys derived from the MLS epoch.
///
/// # Cryptography
///
/// - **Cipher**: XChaCha20-Poly1305 (24-byte nonce, 16-byte tag)
/// - **Key Derivation**: Sender keys are derived from the MLS epoch secret
/// - **Nonce**: Deterministically derived from `(epoch, sender_id, log_index)`
///   to prevent reuse
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedMessage {
    /// Ciphertext (encrypted by sender key)
    pub ciphertext: Vec<u8>,

    /// Authentication tag (16 bytes for Poly1305)
    pub tag: [u8; 16],

    /// Nonce for XChaCha20 (24 bytes)
    pub nonce: [u8; 24],

    /// Optional: Push-Carried Ephemeral Keys (PCEK)
    ///
    /// List of encrypted message keys for specific recipients.
    /// Only included for high-priority messages (DMs, mentions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_keys: Option<Vec<PushKey>>,
}

/// Push-Carried Ephemeral Key for a specific recipient
///
/// For high-priority messages (DMs, mentions), the sender can include encrypted
/// message keys for specific recipients. This allows offline devices to decrypt
/// the message via push notifications without fetching the full MLS key
/// schedule.
///
/// # Security
///
/// - **Perfect Forward Secrecy**: Each message uses an ephemeral X25519
///   keypair. Compromise of long-term keys does not compromise past messages.
///
/// - **Selective Recipients**: Only critical recipients receive push keys.
///   Regular group messages rely on the MLS ratchet tree instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushKey {
    /// Recipient device ID
    pub recipient_id: u64,

    /// Encrypted message key (80 bytes: ephemeral_pk + encrypted_key + tag)
    pub encrypted_key: Vec<u8>,
}

/// Delivery receipt
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// Log index of the message being acknowledged
    pub message_log_index: u64,

    /// Type of receipt (delivered, read, etc.)
    pub kind: ReceiptType,

    /// Timestamp in Unix milliseconds since epoch (UTC)
    pub timestamp: u64,
}

/// Receipt type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiptType {
    /// Message delivered to device
    Delivered,
    /// Message read by user
    Read,
}

/// Message reaction (emoji, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    /// Log index of the message being reacted to
    pub message_log_index: u64,

    /// Reaction content (e.g., emoji)
    pub content: String,

    /// True to add, false to remove
    pub add: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_message_serde() {
        let msg = EncryptedMessage {
            ciphertext: vec![1, 2, 3, 4],
            tag: [0; 16],
            nonce: [0; 24],
            push_keys: None,
        };

        let cbor = ciborium::ser::into_writer(&msg, Vec::new());
        assert!(cbor.is_ok());
    }

    #[test]
    fn receipt_serde() {
        let receipt =
            Receipt { message_log_index: 42, kind: ReceiptType::Read, timestamp: 1234567890 };

        let cbor = ciborium::ser::into_writer(&receipt, Vec::new());
        assert!(cbor.is_ok());
    }
}
