//! Ping-pong protocol test using Sunder frames.
//!
//! This test validates:
//! - Frame serialization/deserialization over the network
//! - Bidirectional communication with proper frame handling
//! - Virtual time advancement in simulation
//! - Basic protocol flow (Ping request -> Pong response)

use sunder_core::{env::Environment, transport::Transport};
use sunder_harness::{SimEnv, SimTransport};
use sunder_proto::{
    Frame, FrameHeader, Opcode, Payload,
    payloads::session::{Hello, HelloReply},
};
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
fn ping_pong_basic() {
    let mut sim = turmoil::Builder::new().build();

    // Server: respond to Ping with Pong
    sim.host("server", || async move {
        let transport = SimTransport::bind("0.0.0.0:443").await?;
        let (mut send, mut recv) = transport.accept().await?;

        // Read frame header (128 bytes)
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;

        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;

        // Verify it's a Ping
        assert_eq!(header.opcode_enum(), Some(Opcode::Ping));

        // Read payload
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
        let env = SimEnv;
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

        // Verify it's a Pong
        assert_eq!(header.opcode_enum(), Some(Opcode::Pong));

        // Read payload (should be empty for Pong)
        let payload_size = header.payload_size() as usize;
        assert_eq!(payload_size, 0, "Pong should have no payload");

        Ok(())
    });

    sim.run().expect("simulation should complete successfully");
}

#[test]
fn ping_pong_with_payload() {
    let mut sim = turmoil::Builder::new().build();

    // Server: echo back Hello message
    sim.host("server", || async move {
        let transport = SimTransport::bind("0.0.0.0:443").await?;
        let (mut send, mut recv) = transport.accept().await?;

        // Read frame
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;
        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;

        let payload_size = header.payload_size() as usize;
        let mut payload_buf = vec![0u8; payload_size];
        recv.read_exact(&mut payload_buf).await?;

        let frame = Frame::new(*header, payload_buf);

        // Parse payload
        let payload = Payload::from_frame(frame).map_err(to_box_err)?;

        // Verify it's a Hello
        match payload {
            Payload::Hello(hello) => {
                assert_eq!(hello.version, 1);

                // Send HelloReply
                let reply = Payload::HelloReply(HelloReply {
                    session_id: 0x1234_5678_9ABC_DEF0,
                    capabilities: vec![],
                    challenge: None,
                });

                let reply_frame =
                    reply.into_frame(create_header(Opcode::HelloReply)).map_err(to_box_err)?;

                let mut reply_buf = Vec::new();
                reply_frame.encode(&mut reply_buf).map_err(to_box_err)?;
                send.write_all(&reply_buf).await?;

                Ok(())
            },
            _ => Err(to_box_err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "expected Hello",
            ))),
        }
    });

    // Client: send Hello, expect HelloReply
    sim.client("client", async {
        let stream = SimTransport::connect_to("server:443").await?;
        let (mut recv, mut send) = tokio::io::split(stream);

        // Create Hello payload
        let hello = Payload::Hello(Hello { version: 1, capabilities: vec![], auth_token: None });

        let hello_frame = hello.into_frame(create_header(Opcode::Hello)).map_err(to_box_err)?;

        // Send Hello
        let mut hello_buf = Vec::new();
        hello_frame.encode(&mut hello_buf).map_err(to_box_err)?;
        send.write_all(&hello_buf).await?;

        // Read HelloReply
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;
        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;

        assert_eq!(header.opcode_enum(), Some(Opcode::HelloReply));

        let payload_size = header.payload_size() as usize;
        let mut payload_buf = vec![0u8; payload_size];
        recv.read_exact(&mut payload_buf).await?;

        let frame = Frame::new(*header, payload_buf);
        let payload = Payload::from_frame(frame).map_err(to_box_err)?;

        // Verify HelloReply
        match payload {
            Payload::HelloReply(reply) => {
                assert_eq!(reply.session_id, 0x1234_5678_9ABC_DEF0);
                Ok(())
            },
            _ => Err(to_box_err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "expected HelloReply",
            ))),
        }
    });

    sim.run().expect("simulation should complete successfully");
}

#[test]
fn ping_pong_multiple_clients() {
    let mut sim = turmoil::Builder::new().build();

    // Server: handle multiple clients
    sim.host("server", || async move {
        let transport = SimTransport::bind("0.0.0.0:443").await?;

        // Accept first client
        let (mut send1, mut recv1) = transport.accept().await?;

        // Accept second client
        let (mut send2, mut recv2) = transport.accept().await?;

        // Handle both clients concurrently
        let task1 = async {
            let mut header_buf = [0u8; FrameHeader::SIZE];
            recv1.read_exact(&mut header_buf).await?;
            let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
            assert_eq!(header.opcode_enum(), Some(Opcode::Ping));

            let pong = Frame::new(create_header(Opcode::Pong), Vec::new());
            let mut buf = Vec::new();
            pong.encode(&mut buf).map_err(to_box_err)?;
            send1.write_all(&buf).await?;
            Ok::<_, Box<dyn std::error::Error>>(())
        };

        let task2 = async {
            let mut header_buf = [0u8; FrameHeader::SIZE];
            recv2.read_exact(&mut header_buf).await?;
            let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
            assert_eq!(header.opcode_enum(), Some(Opcode::Ping));

            let pong = Frame::new(create_header(Opcode::Pong), Vec::new());
            let mut buf = Vec::new();
            pong.encode(&mut buf).map_err(to_box_err)?;
            send2.write_all(&buf).await?;
            Ok::<_, Box<dyn std::error::Error>>(())
        };

        // Run both tasks concurrently
        tokio::try_join!(task1, task2)?;

        Ok(())
    });

    // Client 1
    sim.client("client1", async {
        let stream = SimTransport::connect_to("server:443").await?;
        let (mut recv, mut send) = tokio::io::split(stream);

        let ping = Frame::new(create_header(Opcode::Ping), Vec::new());
        let mut buf = Vec::new();
        ping.encode(&mut buf).map_err(to_box_err)?;
        send.write_all(&buf).await?;

        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;
        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
        assert_eq!(header.opcode_enum(), Some(Opcode::Pong));

        Ok(())
    });

    // Client 2
    sim.client("client2", async {
        let stream = SimTransport::connect_to("server:443").await?;
        let (mut recv, mut send) = tokio::io::split(stream);

        let ping = Frame::new(create_header(Opcode::Ping), Vec::new());
        let mut buf = Vec::new();
        ping.encode(&mut buf).map_err(to_box_err)?;
        send.write_all(&buf).await?;

        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;
        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
        assert_eq!(header.opcode_enum(), Some(Opcode::Pong));

        Ok(())
    });

    sim.run().expect("simulation should complete successfully");
}
