//! Deterministic simulation harness for Sunder protocol testing.
//!
//! This crate provides Turmoil-based implementations of the `Environment`
//! and `Transport` traits, enabling deterministic, reproducible testing
//! of the Sunder protocol under various network conditions.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod sim_env;
mod sim_transport;

pub mod scenario;

pub use sim_env::SimEnv;
pub use sim_transport::SimTransport;
