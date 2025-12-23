//! Transport abstraction for connection-oriented protocols.
//!
//! Abstracts over transports that support multiplexed streams (like QUIC).
//! Production uses Quinn (real QUIC), tests use Turmoil (simulated TCP).

use std::{io, net::SocketAddr};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

/// Abstract transport for connection-oriented protocols with multiplexed
/// streams.
///
/// This trait models QUIC's architecture:
/// - One Connection can have many Streams
/// - Connections are long-lived and have connection-level operations
/// - Streams are cheap, multiplexed, and have stream-level operations
///
/// NOTE: We can't simulate QUIC directly because Quinn doesn't support
/// pluggable time/RNG. But that's fine since Lockframe's protocol logic lives
/// inside streams, so we test protocol correctness, not QUIC reliability.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Type representing a connection to a peer.
    ///
    /// A connection is long-lived and supports:
    /// - Opening new streams
    /// - Accepting incoming streams
    /// - Connection-level close with error code
    type Connection: TransportConnection;

    /// Accept an incoming connection.
    ///
    /// Blocks until a connection is established and returns a Connection
    /// handle.
    async fn accept(&self) -> io::Result<Self::Connection>;

    /// Connect to a remote endpoint.
    ///
    /// Initiates a connection to the remote address, waits for the handshake to
    /// complete, and returns a Connection handle.
    async fn connect(&self, remote: SocketAddr) -> io::Result<Self::Connection>;
}

/// A connection to a remote peer, supporting multiplexed streams.
///
/// Represents a QUIC connection or its simulation equivalent. Multiple streams
/// can be opened/accepted concurrently over a single connection.
#[async_trait]
pub trait TransportConnection: Send + Sync + 'static {
    /// Type of stream for sending data.
    type SendStream: AsyncWrite + Unpin + Send + 'static;

    /// Type of stream for receiving data.
    type RecvStream: AsyncRead + Unpin + Send + 'static;

    /// Open a new bidirectional stream.
    ///
    /// Creates a new stream over this connection and returns send and receive
    /// halves. Stream creation is lightweight (multiplexing).
    async fn open_bi(&self) -> io::Result<(Self::SendStream, Self::RecvStream)>;

    /// Accept an incoming bidirectional stream.
    ///
    /// Blocks until peer opens a stream and returns send and receive halves.
    /// Returns `Ok(None)` if connection is gracefully closed.
    async fn accept_bi(&self) -> io::Result<Option<(Self::SendStream, Self::RecvStream)>>;

    /// Close the connection immediately with an error code.
    ///
    /// Terminates all streams on this connection, sends close frame to peer
    /// with error code, and returns immediately (non-blocking).
    fn close(&self, error_code: u64, reason: &str);
}
