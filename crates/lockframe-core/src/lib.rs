//! Lockframe protocol core logic
//!
//! Pure state machine logic for the Lockframe protocol, completely decoupled
//! from I/O. This enables deterministic testing and formal verification.
//!
//! # Architecture
//!
//! Protocol logic in this crate is implemented as deterministic state
//! machines that are isolated from I/O, time, randomness, and scheduling.
//! All external effects are supplied explicitly by the caller.
//!
//! State transitions produce declarative actions that describe intended
//! effects rather than executing them directly. A runtime or test harness
//! is responsible for interpreting and executing these actions.
//!
//! This separation keeps protocol correctness independent of execution
//! concerns and allows the same code to be reused across production
//! runtimes, deterministic unit tests, and simulation environments with
//! fault injection.
//!
//! # Components
//!
//! - [`connection`]: Connection state machine (handshake, heartbeat, timeout)
//! - [`mls`]: MLS group state machine (proposals, commits, messages)
//! - [`mod@env`]: Environment abstraction (time, RNG)
//! - [`transport`]: Transport abstraction (streams)
//! - [`error`]: Connection error types

pub mod connection;
pub mod env;
pub mod error;
pub mod mls;
pub mod transport;
