//! High-throughput message encryption using Sender Keys.
//!
//! Separates control plane (MLS) from data plane (message encryption). MLS
//! gives us forward secrecy and key agreement, but it's too slow for encrypting
//! every message at 10K+ msg/sec. Instead, we derive per-sender ratchets from
//! each MLS epoch and use those for fast symmetric encryption.
//!
//! Each epoch, MLS gives us an epoch secret. We derive a unique seed for each
//! sender (via HKDF), initialize a symmetric ratchet, and use that to generate
//! message keys. Messages are encrypted with XChaCha20-Poly1305.
//!
//! # Security
//!
//! Forward secrecy comes from MLS epoch rotation. Sender isolation means
//! compromising one sender doesn't expose other senders' messages. AEAD
//! prevents tampering and provides sender authentication.

pub mod derivation;
pub mod encryption;
pub mod error;
pub mod ratchet;

pub use derivation::derive_sender_key_seed;
pub use encryption::{EncryptedMessage, NONCE_RANDOM_SIZE, decrypt_message, encrypt_message};
pub use error::SenderKeyError;
pub use ratchet::{MessageKey, SymmetricRatchet};
