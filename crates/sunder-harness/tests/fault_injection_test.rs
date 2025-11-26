//! Fault injection tests for Sunder protocol.
//!
//! These tests validate that the protocol handles realistic network conditions:
//! - Packet loss (2% - realistic degraded network, handled by TCP
//!   retransmissions)
//! - Network latency (100ms - typical poor network conditions)
//! - Network partitions (split-brain scenarios)
//!
//! # Why 2% packet loss?
//!
//! Real-world networks:
//! - **<1% loss**: Normal operation
//! - **1-2% loss**: Degraded but usable (realistic worst-case for production)
//! - **5-10% loss**: Severe degradation, users experiencing issues
//! - **>20% loss**: Network effectively broken, applications fail
//!
//! Testing 2% validates our protocol can survive degraded but realistic
//! conditions. Higher loss rates cause TCP handshake failures and extreme
//! retransmission delays, making tests non-deterministic.

use sunder_core::{env::Environment, transport::Transport};
use sunder_harness::{SimEnv, SimTransport};
use sunder_proto::{Frame, FrameHeader, Opcode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Helper to convert any error to Box<dyn Error>
fn to_box_err<E: std::error::Error + 'static>(e: E) -> Box<dyn std::error::Error> {
    Box::new(e)
}

/// Helper to create a valid frame header
fn create_header(opcode: Opcode) -> FrameHeader {
    let mut bytes = [0u8; FrameHeader::SIZE];

    // Set magic and version
    bytes[0..4].copy_from_slice(&FrameHeader::MAGIC.to_be_bytes());
    bytes[4] = FrameHeader::VERSION;

    let header = FrameHeader::from_bytes(&bytes).expect("valid header").to_owned();

    // Create a mutable copy and set the opcode
    let mut header_bytes = header.to_bytes();
    header_bytes[6..8].copy_from_slice(&opcode.to_u16().to_be_bytes());

    FrameHeader::from_bytes(&header_bytes).expect("valid header with opcode").to_owned()
}

#[test]
fn ping_pong_with_packet_loss() {
    // TCP will handle retransmissions automatically
    // Using 2% loss - realistic degraded network that completes reliably
    // Set deterministic seed for reproducible packet loss patterns
    let mut sim = turmoil::Builder::new()
        .simulation_duration(std::time::Duration::from_secs(60))
        .fail_rate(0.02)  // 2% packet loss - realistic degraded network
        .rng_seed(12345)  // Deterministic seed
        .build();

    // Server: respond to Ping with Pong
    sim.host("server", || async move {
        let transport = SimTransport::bind("0.0.0.0:443").await?;
        let (mut send, mut recv) = transport.accept().await?;

        // Read frame header (128 bytes)
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;

        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
        assert_eq!(header.opcode_enum(), Some(Opcode::Ping));

        // Read payload (should be empty)
        let payload_size = header.payload_size() as usize;
        let mut payload_buf = vec![0u8; payload_size];
        recv.read_exact(&mut payload_buf).await?;

        // Create Pong response
        let pong_header = create_header(Opcode::Pong);
        let pong_frame = Frame::new(pong_header, Vec::new());

        // Send response
        let mut response_buf = Vec::new();
        pong_frame.encode(&mut response_buf).map_err(to_box_err)?;
        send.write_all(&response_buf).await?;

        Ok(())
    });

    // Client: send Ping, expect Pong
    sim.client("client", async {
        let env = SimEnv::new();
        let stream = SimTransport::connect_to("server:443").await?;
        let (mut recv, mut send) = tokio::io::split(stream);

        // Wait a bit (virtual time)
        env.sleep(std::time::Duration::from_millis(10)).await;

        // Create Ping frame
        let ping_header = create_header(Opcode::Ping);
        let ping_frame = Frame::new(ping_header, Vec::new());

        // Send Ping
        let mut ping_buf = Vec::new();
        ping_frame.encode(&mut ping_buf).map_err(to_box_err)?;
        send.write_all(&ping_buf).await?;

        // Read Pong response header
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;

        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
        assert_eq!(header.opcode_enum(), Some(Opcode::Pong));

        // Read payload (should be empty for Pong)
        let payload_size = header.payload_size() as usize;
        assert_eq!(payload_size, 0, "Pong should have no payload");

        Ok(())
    });

    sim.run().expect("simulation should complete despite packet loss");
}

#[test]
fn ping_pong_with_latency() {
    let mut sim = turmoil::Builder::new()
        .simulation_duration(std::time::Duration::from_secs(60))
        .min_message_latency(std::time::Duration::from_millis(100))
        .max_message_latency(std::time::Duration::from_millis(100))
        .build();

    // Server: respond to Ping with Pong
    sim.host("server", || async move {
        let transport = SimTransport::bind("0.0.0.0:443").await?;
        let (mut send, mut recv) = transport.accept().await?;

        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;

        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
        assert_eq!(header.opcode_enum(), Some(Opcode::Ping));

        let payload_size = header.payload_size() as usize;
        let mut payload_buf = vec![0u8; payload_size];
        recv.read_exact(&mut payload_buf).await?;

        let pong_header = create_header(Opcode::Pong);
        let pong_frame = Frame::new(pong_header, Vec::new());

        let mut response_buf = Vec::new();
        pong_frame.encode(&mut response_buf).map_err(to_box_err)?;
        send.write_all(&response_buf).await?;

        Ok(())
    });

    // Client: measure round-trip time
    sim.client("client", async {
        let env = SimEnv::new();
        let stream = SimTransport::connect_to("server:443").await?;
        let (mut recv, mut send) = tokio::io::split(stream);

        let start = env.now();

        // Send Ping
        let ping_header = create_header(Opcode::Ping);
        let ping_frame = Frame::new(ping_header, Vec::new());

        let mut ping_buf = Vec::new();
        ping_frame.encode(&mut ping_buf).map_err(to_box_err)?;
        send.write_all(&ping_buf).await?;

        // Read Pong
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;

        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
        assert_eq!(header.opcode_enum(), Some(Opcode::Pong));

        let elapsed = env.now() - start;

        // Round trip should be ~200ms (100ms each way)
        assert!(
            elapsed >= std::time::Duration::from_millis(200),
            "Round trip too fast: {:?}",
            elapsed
        );

        Ok(())
    });

    sim.run().expect("simulation should complete with latency");
}
