//! Connection state machine for Sunder protocol.
//!
//! This module implements the session layer - managing connection lifecycle,
//! heartbeats, timeouts, and graceful shutdown.
//!
//! # Architecture: Action-Based State Machine
//!
//! This state machine follows the action pattern:
//! - Methods accept time as parameter (no stored Environment)
//! - Methods return `Result<Vec<ConnectionAction>, ConnectionError>`
//! - Driver code executes actions (send frames, close connection, etc.)
//!
//! This enables:
//! - Pure state machine logic (no I/O)
//! - Easy testing (no mocking time/RNG)
//! - Composability (multiple connections can share one Environment)
//!
//! # State Machine
//!
//! ```text
//! ┌──────┐  Hello   ┌──────────┐  Authenticated  ┌──────────────┐
//! │ Init │─────────>│ Pending  │───────────────>│ Authenticated │
//! └──────┘          └──────────┘                 └──────────────┘
//!                        │                              │
//!                        │ Timeout/Error                │ Goodbye/Timeout
//!                        ↓                              ↓
//!                   ┌────────┐                     ┌────────┐
//!                   │ Closed │<────────────────────│ Closed │
//!                   └────────┘                     └────────┘
//! ```
//!
//! # Lifecycle
//!
//! 1. **Init**: Connection created, no handshake yet
//! 2. **Pending**: Hello sent, waiting for HelloReply
//! 3. **Authenticated**: HelloReply received, ready for messages
//! 4. **Closed**: Connection terminated (graceful or error)
//!
//! # Timeouts
//!
//! - **Handshake timeout**: 30 seconds to complete Hello/HelloReply
//! - **Idle timeout**: 60 seconds without any activity
//! - **Heartbeat interval**: 20 seconds (sends Ping to keep alive)

use std::time::{Duration, Instant};

use sunder_proto::Frame;

use crate::error::ConnectionError;

/// Actions returned by the connection state machine.
///
/// The driver (test harness or production server) executes these actions:
/// - `SendFrame`: Serialize and send the frame over the transport
/// - `Close`: Close the connection with the given reason
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionAction {
    /// Send this frame to the peer
    SendFrame(Frame),

    /// Close the connection with this reason
    Close {
        /// Reason for closing the connection
        reason: String,
    },
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state - no handshake started
    Init,
    /// Hello sent, waiting for HelloReply
    Pending,
    /// HelloReply received, connection authenticated
    Authenticated,
    /// Connection closed (graceful or error)
    Closed,
}

/// Connection configuration
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Timeout for completing handshake
    pub handshake_timeout: Duration,
    /// Idle timeout before disconnecting
    pub idle_timeout: Duration,
    /// Heartbeat interval (should be < idle_timeout / 2)
    pub heartbeat_interval: Duration,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            heartbeat_interval: Duration::from_secs(20),
        }
    }
}

/// Connection state machine
///
/// Manages lifecycle, timeouts, and heartbeats for a single connection.
///
/// This is a pure state machine - no I/O, no Environment storage.
/// Time is passed as parameters to methods that need it.
#[derive(Debug, Clone)]
pub struct Connection {
    /// Current state
    state: ConnectionState,
    /// Configuration
    config: ConnectionConfig,
    /// Last activity timestamp
    last_activity: Instant,
    /// Last heartbeat sent timestamp
    last_heartbeat: Option<Instant>,
    /// Session ID (assigned by server)
    session_id: Option<u64>,
}

impl Connection {
    /// Create a new connection in Init state
    ///
    /// # Arguments
    /// * `now` - Current time (from Environment)
    /// * `config` - Connection configuration
    pub fn new(now: Instant, config: ConnectionConfig) -> Self {
        Self {
            state: ConnectionState::Init,
            config,
            last_activity: now,
            last_heartbeat: None,
            session_id: None,
        }
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Get session ID (if authenticated)
    #[must_use]
    pub fn session_id(&self) -> Option<u64> {
        self.session_id
    }

    /// Transition to Pending state (Hello sent)
    ///
    /// Returns actions to execute (send Hello frame, update activity).
    ///
    /// # Arguments
    /// * `now` - Current time
    ///
    /// # Errors
    /// Returns `InvalidState` if not in Init state
    pub fn send_hello(&mut self, now: Instant) -> Result<Vec<ConnectionAction>, ConnectionError> {
        if self.state != ConnectionState::Init {
            return Err(ConnectionError::InvalidState {
                state: self.state,
                operation: "send_hello".to_string(),
            });
        }

        self.state = ConnectionState::Pending;
        self.last_activity = now;

        // Note: The actual Hello frame will be created by the driver
        // This method just manages state transitions
        Ok(vec![])
    }

    /// Transition to Authenticated state (HelloReply received)
    ///
    /// # Arguments
    /// * `session_id` - Session ID assigned by server
    /// * `now` - Current time
    ///
    /// # Errors
    /// Returns `InvalidState` if not in Pending state
    pub fn receive_hello_reply(
        &mut self,
        session_id: u64,
        now: Instant,
    ) -> Result<Vec<ConnectionAction>, ConnectionError> {
        if self.state != ConnectionState::Pending {
            return Err(ConnectionError::InvalidState {
                state: self.state,
                operation: "receive_hello_reply".to_string(),
            });
        }

        self.state = ConnectionState::Authenticated;
        self.session_id = Some(session_id);
        self.last_activity = now;

        Ok(vec![])
    }

    /// Transition to Closed state
    pub fn close(&mut self) {
        self.state = ConnectionState::Closed;
    }

    /// Update last activity timestamp
    ///
    /// Call this when receiving any frame from peer.
    pub fn update_activity(&mut self, now: Instant) {
        self.last_activity = now;
    }

    /// Check if connection has timed out
    ///
    /// # Arguments
    /// * `now` - Current time
    ///
    /// # Returns
    /// `Some(elapsed)` if timed out, `None` otherwise
    #[must_use]
    pub fn check_timeout(&self, now: Instant) -> Option<Duration> {
        let elapsed = now.duration_since(self.last_activity);

        let timeout = match self.state {
            ConnectionState::Pending => self.config.handshake_timeout,
            ConnectionState::Authenticated => self.config.idle_timeout,
            _ => return None,
        };

        if elapsed > timeout { Some(elapsed) } else { None }
    }

    /// Tick the state machine - check for timeouts and heartbeats
    ///
    /// Call this periodically (e.g., every 100ms) to handle:
    /// - Timeout detection
    /// - Heartbeat sending
    ///
    /// # Arguments
    /// * `now` - Current time
    ///
    /// # Returns
    /// Actions to execute (send Ping, close connection, etc.)
    pub fn tick(&mut self, now: Instant) -> Vec<ConnectionAction> {
        let mut actions = Vec::new();

        // Check for timeout
        if let Some(elapsed) = self.check_timeout(now) {
            let reason = match self.state {
                ConnectionState::Pending => format!("handshake timeout after {:?}", elapsed),
                ConnectionState::Authenticated => format!("idle timeout after {:?}", elapsed),
                _ => "timeout".to_string(),
            };

            self.close();
            actions.push(ConnectionAction::Close { reason });
            return actions;
        }

        // Check if we should send heartbeat
        if self.state == ConnectionState::Authenticated {
            let should_send = match self.last_heartbeat {
                None => true, // Never sent heartbeat
                Some(last) => {
                    let elapsed = now.duration_since(last);
                    elapsed >= self.config.heartbeat_interval
                },
            };

            if should_send {
                // Create Ping frame - use helper to create header with opcode
                let mut header_bytes = [0u8; sunder_proto::FrameHeader::SIZE];
                header_bytes[0..4].copy_from_slice(&sunder_proto::FrameHeader::MAGIC.to_be_bytes());
                header_bytes[4] = sunder_proto::FrameHeader::VERSION;

                let header = sunder_proto::FrameHeader::from_bytes(&header_bytes)
                    .expect("valid header")
                    .to_owned();
                let mut header_bytes = header.to_bytes();
                header_bytes[6..8]
                    .copy_from_slice(&sunder_proto::Opcode::Ping.to_u16().to_be_bytes());

                let ping_header = sunder_proto::FrameHeader::from_bytes(&header_bytes)
                    .expect("valid ping header")
                    .to_owned();
                let ping_frame = Frame::new(ping_header, Vec::new());

                actions.push(ConnectionAction::SendFrame(ping_frame));
                self.last_heartbeat = Some(now);
                self.last_activity = now;
            }
        }

        actions
    }

    /// Handle incoming frame
    ///
    /// Process a frame received from the peer and return actions.
    ///
    /// # Arguments
    /// * `frame` - The received frame
    /// * `now` - Current time
    ///
    /// # Returns
    /// Actions to execute in response
    ///
    /// # Errors
    /// Returns error if frame is unexpected for current state
    pub fn handle_frame(
        &mut self,
        frame: &Frame,
        now: Instant,
    ) -> Result<Vec<ConnectionAction>, ConnectionError> {
        // Update activity on any frame received
        self.last_activity = now;

        // Handle based on current state and frame type
        match (self.state, frame.header.opcode_enum()) {
            (ConnectionState::Authenticated, Some(sunder_proto::Opcode::Pong)) => {
                // Pong received - just update activity (already done above)
                Ok(vec![])
            },
            (state, Some(opcode)) => {
                Err(ConnectionError::UnexpectedFrame { state, opcode: opcode.to_u16() })
            },
            (state, None) => {
                Err(ConnectionError::UnexpectedFrame { state, opcode: frame.header.opcode() })
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_lifecycle() {
        let t0 = Instant::now();
        let mut conn = Connection::new(t0, ConnectionConfig::default());

        // Initial state
        assert_eq!(conn.state(), ConnectionState::Init);
        assert_eq!(conn.session_id(), None);

        // Send Hello
        let actions = conn.send_hello(t0).unwrap();
        assert_eq!(conn.state(), ConnectionState::Pending);
        assert!(actions.is_empty()); // State transition only

        // Receive HelloReply
        let actions = conn.receive_hello_reply(12345, t0).unwrap();
        assert_eq!(conn.state(), ConnectionState::Authenticated);
        assert_eq!(conn.session_id(), Some(12345));
        assert!(actions.is_empty());

        // Close
        conn.close();
        assert_eq!(conn.state(), ConnectionState::Closed);
    }

    #[test]
    fn heartbeat_timing() {
        let t0 = Instant::now();
        let config =
            ConnectionConfig { heartbeat_interval: Duration::from_secs(20), ..Default::default() };
        let mut conn = Connection::new(t0, config);

        // Move to authenticated
        conn.send_hello(t0).unwrap();
        conn.receive_hello_reply(12345, t0).unwrap();

        // Tick immediately - should send first heartbeat
        let t1 = t0;
        let actions = conn.tick(t1);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ConnectionAction::SendFrame(_)));

        // Tick again immediately - should NOT send (too soon)
        let t2 = t1 + Duration::from_secs(1);
        let actions = conn.tick(t2);
        assert!(actions.is_empty());

        // Advance time past heartbeat interval
        let t3 = t1 + Duration::from_secs(21);
        let actions = conn.tick(t3);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ConnectionAction::SendFrame(_)));
    }

    #[test]
    fn handshake_timeout() {
        let t0 = Instant::now();
        let config =
            ConnectionConfig { handshake_timeout: Duration::from_secs(30), ..Default::default() };
        let mut conn = Connection::new(t0, config);

        conn.send_hello(t0).unwrap();

        // Check timeout immediately - should be fine
        assert!(conn.check_timeout(t0).is_none());

        // Advance time past handshake timeout
        let t1 = t0 + Duration::from_secs(31);
        let elapsed = conn.check_timeout(t1);
        assert!(elapsed.is_some());
        assert!(elapsed.unwrap() > Duration::from_secs(30));

        // Tick should close connection
        let actions = conn.tick(t1);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ConnectionAction::Close { .. }));
        assert_eq!(conn.state(), ConnectionState::Closed);
    }

    #[test]
    fn idle_timeout() {
        let t0 = Instant::now();
        let config =
            ConnectionConfig { idle_timeout: Duration::from_secs(60), ..Default::default() };
        let mut conn = Connection::new(t0, config);

        // Move to authenticated
        conn.send_hello(t0).unwrap();
        conn.receive_hello_reply(12345, t0).unwrap();

        // Check timeout immediately - should be fine
        assert!(conn.check_timeout(t0).is_none());

        // Advance time past idle timeout
        let t1 = t0 + Duration::from_secs(61);
        let elapsed = conn.check_timeout(t1);
        assert!(elapsed.is_some());

        // Tick should close connection
        let actions = conn.tick(t1);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ConnectionAction::Close { .. }));
        assert_eq!(conn.state(), ConnectionState::Closed);
    }

    #[test]
    fn invalid_state_transitions() {
        let t0 = Instant::now();
        let mut conn = Connection::new(t0, ConnectionConfig::default());

        // Can't receive HelloReply from Init
        let result = conn.receive_hello_reply(12345, t0);
        assert!(matches!(result, Err(ConnectionError::InvalidState { .. })));

        // Can't send Hello twice
        conn.send_hello(t0).unwrap();
        let result = conn.send_hello(t0);
        assert!(matches!(result, Err(ConnectionError::InvalidState { .. })));
    }

    #[test]
    fn handle_pong_updates_activity() {
        let t0 = Instant::now();
        let mut conn = Connection::new(t0, ConnectionConfig::default());

        // Move to authenticated
        conn.send_hello(t0).unwrap();
        conn.receive_hello_reply(12345, t0).unwrap();

        // Create a Pong frame
        let mut header_bytes = [0u8; sunder_proto::FrameHeader::SIZE];
        header_bytes[0..4].copy_from_slice(&sunder_proto::FrameHeader::MAGIC.to_be_bytes());
        header_bytes[4] = sunder_proto::FrameHeader::VERSION;

        let header =
            sunder_proto::FrameHeader::from_bytes(&header_bytes).expect("valid header").to_owned();
        let mut header_bytes = header.to_bytes();
        header_bytes[6..8].copy_from_slice(&sunder_proto::Opcode::Pong.to_u16().to_be_bytes());

        let pong_header = sunder_proto::FrameHeader::from_bytes(&header_bytes)
            .expect("valid pong header")
            .to_owned();
        let pong_frame = Frame::new(pong_header, Vec::new());

        // Handle Pong
        let t1 = t0 + Duration::from_secs(30);
        let actions = conn.handle_frame(&pong_frame, t1).unwrap();
        assert!(actions.is_empty());

        // Activity should be updated (not timed out)
        let t2 = t1 + Duration::from_secs(40); // 40s after Pong, but only 10s from last activity
        assert!(conn.check_timeout(t2).is_none());
    }
}
