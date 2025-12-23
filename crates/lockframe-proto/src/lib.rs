//! Wire format for the Lockframe protocol.
//!
//! Frames consist of a fixed 128-byte header (zero-copy binary) followed by a
//! variable-length CBOR payload. The header contains routing and sequencing
//! information, while the payload carries the actual protocol messages.
//!
//! We chose this hybrid approach because the sequencer needs to make routing
//! decisions at high throughput (15K+ frames/sec) without deserializing
//! payloads. The 128-byte header fits in two cache lines, so we can route
//! frames by only touching 64 bytes of memory.
//!
//! # Security
//!
//! All parsing uses compile-time verified layouts via `zerocopy`. We enforce a
//! 16 MB payload limit to prevent memory exhaustion attacks. No "fast paths"
//! that skip validation.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod errors;
pub mod flags;
pub mod frame;
pub mod header;
pub mod opcodes;
pub mod payloads;

pub use errors::{ProtocolError, Result};
pub use flags::FrameFlags;
pub use frame::Frame;
pub use header::FrameHeader;
pub use opcodes::Opcode;
pub use payloads::Payload;
