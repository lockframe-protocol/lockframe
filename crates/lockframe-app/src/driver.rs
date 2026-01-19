//! Driver trait for abstracting I/O operations.
//!
//! The [`Driver`] trait decouples the application runtime from specific I/O
//! implementations. Each frontend implements the trait to provide
//! platform-specific I/O, while the generic [`crate::Runtime`] handles all
//! orchestration.

use std::{future::Future, ops::Sub, time::Duration};

use lockframe_proto::Frame;

use crate::{App, AppAction};

/// Abstracts I/O operations for the application runtime.
///
/// Implementations provide platform-specific I/O while the generic
/// [`crate::Runtime`] handles orchestration logic. This ensures
/// the same orchestration code runs in production TUI and simulation.
pub trait Driver: Send {
    /// Platform-specific error type.
    type Error: std::error::Error + Send + 'static;

    /// Time instant type. Enables virtual time in simulation.
    type Instant: Copy + Ord + Send + Sync + Sub<Output = Duration>;

    /// Poll for input and return actions to process.
    ///
    /// Returns empty vector if no input is ready.
    fn poll_event(
        &mut self,
        app: &mut App,
    ) -> impl Future<Output = Result<Vec<AppAction>, Self::Error>> + Send;

    /// Send a frame to the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection is closed or send fails.
    fn send_frame(&mut self, frame: Frame) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Receive a frame from the server.
    ///
    /// Returns frame or `None` if the connection is closed.
    fn recv_frame(&mut self) -> impl Future<Output = Option<Frame>> + Send;

    /// Establish connection to the server.
    ///
    /// # Errors
    ///
    /// Returns an error if connection cannot be established.
    fn connect(&mut self, addr: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Check if connected to server.
    fn is_connected(&self) -> bool;

    /// Current time instant.
    fn now(&self) -> Self::Instant;

    /// Render the application state.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering fails.
    fn render(&mut self, app: &App) -> Result<(), Self::Error>;

    /// Stop the connection and clean up resources.
    fn stop(&mut self);
}
