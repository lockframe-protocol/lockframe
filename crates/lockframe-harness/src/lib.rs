//! Deterministic simulation harness for Lockframe protocol testing.
//!
//! Turmoil-based implementations of the Environment and Transport traits for
//! deterministic, reproducible testing under various network conditions.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod scenario;
pub mod sim_env;
pub mod sim_server;
pub mod sim_transport;

pub use sim_env::SimEnv;
pub use sim_server::{SharedSimServer, SimServer, create_shared_server};
pub use sim_transport::SimTransport;
