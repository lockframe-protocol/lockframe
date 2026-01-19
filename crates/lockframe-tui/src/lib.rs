//! Terminal UI for Lockframe
//!
//! A thin shell over [`lockframe_app::Driver`] that provides terminal-specific
//! I/O. All orchestration logic lives in the generic [`lockframe_app::Runtime`]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod commands;
pub mod input;
pub mod terminal;
pub mod ui;

pub use commands::Command;
pub use input::{InputState, KeyInput};
pub use lockframe_app::{App, AppAction, AppEvent, Bridge, Driver, Runtime};
pub use terminal::{TerminalDriver, TerminalError};
