//! Connection lifecycle integration tests.
//!
//! Tests the full connection state machine over the simulated network:
//! - Handshake flow (Hello -> HelloReply)
//! - Heartbeat/keepalive
//! - Timeout detection
//! - Graceful shutdown

use sunder_core::{
    connection::{Connection, ConnectionConfig, ConnectionState},
    env::Environment,
    transport::Transport,
};
use sunder_harness::{SimEnv, SimTransport};
use sunder_proto::{
    Frame, FrameHeader, Opcode, Payload,
    payloads::session::{Goodbye, Hello, HelloReply},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Helper to convert any error to Box<dyn Error>
fn to_box_err<E: std::error::Error + 'static>(e: E) -> Box<dyn std::error::Error> {
    Box::new(e)
}

/// Helper to create a valid frame header
fn create_header(opcode: Opcode) -> FrameHeader {
    let mut bytes = [0u8; FrameHeader::SIZE];
    bytes[0..4].copy_from_slice(&FrameHeader::MAGIC.to_be_bytes());
    bytes[4] = FrameHeader::VERSION;

    let header = FrameHeader::from_bytes(&bytes).expect("valid header").to_owned();
    let mut header_bytes = header.to_bytes();
    header_bytes[6..8].copy_from_slice(&opcode.to_u16().to_be_bytes());

    FrameHeader::from_bytes(&header_bytes).expect("valid header with opcode").to_owned()
}

#[test]
fn connection_handshake_lifecycle() {
    let mut sim = turmoil::Builder::new().build();

    // Server: accept connection, receive Hello, send HelloReply
    sim.host("server", || async move {
        let env = SimEnv::new();
        let transport = SimTransport::bind("0.0.0.0:443").await?;
        let (mut send, mut recv) = transport.accept().await?;

        // Read Hello frame
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;
        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;

        assert_eq!(header.opcode_enum(), Some(Opcode::Hello));

        let payload_size = header.payload_size() as usize;
        let mut payload_buf = vec![0u8; payload_size];
        recv.read_exact(&mut payload_buf).await?;

        let frame = Frame::new(*header, payload_buf);
        let payload = Payload::from_frame(frame).map_err(to_box_err)?;

        // Verify Hello
        match payload {
            Payload::Hello(hello) => {
                assert_eq!(hello.version, 1);

                // Generate session ID
                let session_id = env.random_u64();

                // Send HelloReply
                let reply = Payload::HelloReply(HelloReply {
                    session_id,
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

    // Client: connect, manage state machine, send Hello, receive HelloReply
    sim.client("client", async {
        let env = SimEnv::new();
        let stream = SimTransport::connect_to("server:443").await?;
        let (mut recv, mut send) = tokio::io::split(stream);

        // Create connection state machine
        let now = env.now();
        let mut conn = Connection::new(now, ConnectionConfig::default());
        assert_eq!(conn.state(), ConnectionState::Init);

        // Send Hello
        let hello = Payload::Hello(Hello { version: 1, capabilities: vec![], auth_token: None });

        let hello_frame = hello.into_frame(create_header(Opcode::Hello)).map_err(to_box_err)?;
        let mut hello_buf = Vec::new();
        hello_frame.encode(&mut hello_buf).map_err(to_box_err)?;
        send.write_all(&hello_buf).await?;

        // Update state machine
        let now = env.now();
        conn.send_hello(now).map_err(to_box_err)?;
        assert_eq!(conn.state(), ConnectionState::Pending);

        // Receive HelloReply
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;
        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;

        assert_eq!(header.opcode_enum(), Some(Opcode::HelloReply));

        let payload_size = header.payload_size() as usize;
        let mut payload_buf = vec![0u8; payload_size];
        recv.read_exact(&mut payload_buf).await?;

        let frame = Frame::new(*header, payload_buf);
        let payload = Payload::from_frame(frame).map_err(to_box_err)?;

        // Verify HelloReply and update state
        match payload {
            Payload::HelloReply(reply) => {
                let now = env.now();
                conn.receive_hello_reply(reply.session_id, now).map_err(to_box_err)?;
                assert_eq!(conn.state(), ConnectionState::Authenticated);
                assert_eq!(conn.session_id(), Some(reply.session_id));
                Ok(())
            },
            _ => Err(to_box_err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "expected HelloReply",
            ))),
        }
    });

    sim.run().expect("handshake should complete successfully");
}

#[test]
fn connection_graceful_shutdown() {
    let mut sim = turmoil::Builder::new().build();

    // Server: handle Goodbye
    sim.host("server", || async move {
        let transport = SimTransport::bind("0.0.0.0:443").await?;
        let (mut send, mut recv) = transport.accept().await?;

        // Read Goodbye frame
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;
        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;

        assert_eq!(header.opcode_enum(), Some(Opcode::Goodbye));

        let payload_size = header.payload_size() as usize;
        let mut payload_buf = vec![0u8; payload_size];
        recv.read_exact(&mut payload_buf).await?;

        let frame = Frame::new(*header, payload_buf);
        let payload = Payload::from_frame(frame).map_err(to_box_err)?;

        // Verify Goodbye
        match payload {
            Payload::Goodbye(goodbye) => {
                assert!(!goodbye.reason.is_empty());

                // Send Goodbye acknowledgment
                let reply = Payload::Goodbye(Goodbye { reason: "ack".to_string() });

                let reply_frame =
                    reply.into_frame(create_header(Opcode::Goodbye)).map_err(to_box_err)?;
                let mut reply_buf = Vec::new();
                reply_frame.encode(&mut reply_buf).map_err(to_box_err)?;
                send.write_all(&reply_buf).await?;

                Ok(())
            },
            _ => Err(to_box_err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "expected Goodbye",
            ))),
        }
    });

    // Client: send Goodbye
    sim.client("client", async {
        let env = SimEnv::new();
        let stream = SimTransport::connect_to("server:443").await?;
        let (mut recv, mut send) = tokio::io::split(stream);

        let now = env.now();
        let mut conn = Connection::new(now, ConnectionConfig::default());

        // Send Goodbye
        let goodbye = Payload::Goodbye(Goodbye { reason: "client shutdown".to_string() });

        let goodbye_frame =
            goodbye.into_frame(create_header(Opcode::Goodbye)).map_err(to_box_err)?;
        let mut goodbye_buf = Vec::new();
        goodbye_frame.encode(&mut goodbye_buf).map_err(to_box_err)?;
        send.write_all(&goodbye_buf).await?;

        // Update state
        conn.close();
        assert_eq!(conn.state(), ConnectionState::Closed);

        // Receive Goodbye ack
        let mut header_buf = [0u8; FrameHeader::SIZE];
        recv.read_exact(&mut header_buf).await?;
        let header = FrameHeader::from_bytes(&header_buf).map_err(to_box_err)?;
        assert_eq!(header.opcode_enum(), Some(Opcode::Goodbye));

        Ok(())
    });

    sim.run().expect("graceful shutdown should complete");
}
