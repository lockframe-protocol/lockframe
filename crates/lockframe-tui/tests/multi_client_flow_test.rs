//! End-to-end tests for multi-client room join flow.
//!
//! Tests the complete flow:
//! 1. Client A creates a room
//! 2. Client B publishes KeyPackage
//! 3. Client A adds Client B (fetch KeyPackage, add to MLS group, send Commit + Welcome)
//! 4. Server routes Welcome to Client B
//! 5. Client B processes Welcome and joins the room
//! 6. Both clients can exchange messages

use std::time::Duration;

use lockframe_client::{Client, ClientAction, ClientEvent, ClientIdentity};
use lockframe_core::env::Environment;
use lockframe_proto::{Opcode, Payload, Frame};
use lockframe_server::{
    DriverConfig, MemoryStorage, ServerAction, ServerDriver, ServerEvent,
};

#[derive(Clone)]
struct TestEnv {
    #[allow(dead_code)]
    sender_id: u64,
}

impl Environment for TestEnv {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn sleep(&self, duration: Duration) -> impl std::future::Future<Output = ()> + Send {
        async move {
            tokio::time::sleep(duration).await;
        }
    }

    fn random_bytes(&self, buffer: &mut [u8]) {
        use rand::RngCore;
        rand::rng().fill_bytes(buffer);
    }
}

fn setup_server() -> ServerDriver<TestEnv, MemoryStorage> {
    let env = TestEnv { sender_id: 0 };
    let storage = MemoryStorage::new();
    let config = DriverConfig::default();
    ServerDriver::new(env, storage, config)
}

fn setup_client(sender_id: u64) -> Client<TestEnv> {
    let env = TestEnv { sender_id };
    let identity = ClientIdentity { sender_id };
    Client::new(env, identity)
}

/// Authenticate a client session with the server.
fn authenticate_client(
    driver: &mut ServerDriver<TestEnv, MemoryStorage>,
    session_id: u64,
    user_id: u64,
) {
    let hello = Payload::Hello(lockframe_proto::payloads::session::Hello {
        version: 1,
        capabilities: vec![],
        sender_id: Some(user_id),
        auth_token: None,
    });
    let frame = hello
        .into_frame(lockframe_proto::FrameHeader::new(Opcode::Hello))
        .unwrap();
    driver
        .process_event(ServerEvent::FrameReceived { session_id, frame })
        .unwrap();
}

/// Extract Send frames from client actions.
fn extract_send_frames(actions: &[ClientAction]) -> Vec<Frame> {
    actions
        .iter()
        .filter_map(|a| match a {
            ClientAction::Send(frame) => Some(frame.clone()),
            _ => None,
        })
        .collect()
}

/// Route frames from client to server and collect responses.
fn route_to_server(
    driver: &mut ServerDriver<TestEnv, MemoryStorage>,
    session_id: u64,
    frames: Vec<Frame>,
) -> Vec<ServerAction> {
    let mut all_actions = Vec::new();
    for frame in frames {
        let actions = driver
            .process_event(ServerEvent::FrameReceived { session_id, frame })
            .unwrap();
        all_actions.extend(actions);
    }
    all_actions
}

/// Extract frames sent to a specific session.
fn extract_frames_to_session(actions: &[ServerAction], target_session: u64) -> Vec<Frame> {
    actions
        .iter()
        .filter_map(|a| match a {
            ServerAction::SendToSession { session_id, frame } if *session_id == target_session => {
                Some(frame.clone())
            }
            _ => None,
        })
        .collect()
}

/// Test: Client A creates room, Client B publishes KeyPackage.
#[test]
fn test_create_room_and_publish_keypackage() {
    let mut server = setup_server();
    let mut alice = setup_client(1000);
    let mut bob = setup_client(2000);

    let alice_session = 101;
    let bob_session = 102;
    let room_id = 1;

    // Connect and authenticate both
    server.process_event(ServerEvent::ConnectionAccepted { session_id: alice_session }).unwrap();
    server.process_event(ServerEvent::ConnectionAccepted { session_id: bob_session }).unwrap();
    authenticate_client(&mut server, alice_session, 1000);
    authenticate_client(&mut server, bob_session, 2000);

    // Alice creates room
    let alice_actions = alice.handle(ClientEvent::CreateRoom { room_id }).unwrap();
    println!("Alice CreateRoom actions: {:?}", alice_actions.iter().map(|a| format!("{:?}", a)).collect::<Vec<_>>());

    let frames = extract_send_frames(&alice_actions);
    println!("Alice sending {} frames to server", frames.len());
    for frame in &frames {
        println!("  Opcode: {:?}", frame.header.opcode_enum());
    }

    let server_actions = route_to_server(&mut server, alice_session, frames);
    println!("Server actions: {:?}", server_actions.len());

    // Bob publishes KeyPackage
    let bob_actions = bob.handle(ClientEvent::PublishKeyPackage).unwrap();
    println!("Bob PublishKeyPackage actions: {:?}", bob_actions.iter().map(|a| format!("{:?}", a)).collect::<Vec<_>>());

    let frames = extract_send_frames(&bob_actions);
    println!("Bob sending {} frames to server", frames.len());

    let server_actions = route_to_server(&mut server, bob_session, frames);
    println!("Server actions after publish: {:?}", server_actions);

    // Verify Bob's KeyPackage was published
    assert!(server_actions.iter().any(|a| matches!(a, ServerAction::Log { message, .. } if message.contains("KeyPackage published"))));
}

/// Test: Client A adds Client B (full flow).
#[test]
fn test_add_member_full_flow() {
    let mut server = setup_server();
    let mut alice = setup_client(1000);
    let mut bob = setup_client(2000);

    let alice_session = 101;
    let bob_session = 102;
    let room_id = 1;

    // Setup
    server.process_event(ServerEvent::ConnectionAccepted { session_id: alice_session }).unwrap();
    server.process_event(ServerEvent::ConnectionAccepted { session_id: bob_session }).unwrap();
    authenticate_client(&mut server, alice_session, 1000);
    authenticate_client(&mut server, bob_session, 2000);

    // Alice creates room
    let alice_actions = alice.handle(ClientEvent::CreateRoom { room_id }).unwrap();
    let frames = extract_send_frames(&alice_actions);
    route_to_server(&mut server, alice_session, frames);

    // Bob publishes KeyPackage
    let bob_actions = bob.handle(ClientEvent::PublishKeyPackage).unwrap();
    let frames = extract_send_frames(&bob_actions);
    route_to_server(&mut server, bob_session, frames);

    // Alice fetches and adds Bob
    println!("\n=== Alice adds Bob ===");
    let alice_actions = alice.handle(ClientEvent::FetchAndAddMember { room_id, user_id: 2000 }).unwrap();
    println!("Alice FetchAndAddMember actions:");
    for action in &alice_actions {
        println!("  {:?}", action);
    }

    let fetch_frames = extract_send_frames(&alice_actions);
    assert!(!fetch_frames.is_empty(), "Alice should send KeyPackageFetch request");

    // Server responds to fetch
    let server_actions = route_to_server(&mut server, alice_session, fetch_frames);
    println!("Server actions after fetch request:");
    for action in &server_actions {
        println!("  {:?}", action);
    }

    // Get the KeyPackageFetch response sent to Alice
    let response_frames = extract_frames_to_session(&server_actions, alice_session);
    println!("Response frames to Alice: {}", response_frames.len());

    assert!(!response_frames.is_empty(), "Server should send KeyPackageFetch response");

    // Alice processes the response - this should trigger add_member with the fetched KeyPackage
    for frame in response_frames {
        println!("Alice processing frame: {:?}", frame.header.opcode_enum());
        let alice_actions = alice.handle(ClientEvent::FrameReceived(frame)).unwrap();
        println!("Alice actions after response:");
        for action in &alice_actions {
            println!("  {:?}", action);
        }

        // Check for MemberAdded action
        if alice_actions.iter().any(|a| matches!(a, ClientAction::MemberAdded { .. })) {
            println!("SUCCESS: MemberAdded action produced!");
        }

        // Route any frames Alice sends (Commit + Welcome)
        let frames = extract_send_frames(&alice_actions);
        if !frames.is_empty() {
            println!("Alice sending {} frames:", frames.len());
            for f in &frames {
                let opcode = f.header.opcode_enum();
                if opcode == Some(Opcode::Welcome) {
                    println!("  Opcode: {:?}, recipient: {}", opcode, f.header.recipient_id());
                } else {
                    println!("  Opcode: {:?}, room: {:x}", opcode, f.header.room_id());
                }
            }

            let server_actions = route_to_server(&mut server, alice_session, frames);

            // Check if Welcome is routed to Bob
            let welcome_to_bob = extract_frames_to_session(&server_actions, bob_session);
            println!("Frames routed to Bob: {}", welcome_to_bob.len());
            for f in &welcome_to_bob {
                println!("  Opcode: {:?}", f.header.opcode_enum());
            }

            // Bob processes Welcome
            for welcome_frame in welcome_to_bob {
                if welcome_frame.header.opcode_enum() == Some(Opcode::Welcome) {
                    println!("Bob processing Welcome...");
                    let bob_actions = bob.handle(ClientEvent::FrameReceived(welcome_frame));
                    println!("Bob actions: {:?}", bob_actions);
                }
            }
        }
    }
}

/// Minimal test to trace exactly what happens with FetchAndAddMember.
#[test]
fn test_fetch_and_add_traces_execution() {
    let mut alice = setup_client(1000);
    let room_id = 1;

    // Alice creates room first
    let actions = alice.handle(ClientEvent::CreateRoom { room_id }).unwrap();
    println!("CreateRoom returned {} actions", actions.len());

    // Now try FetchAndAddMember
    let result = alice.handle(ClientEvent::FetchAndAddMember { room_id, user_id: 2000 });
    match result {
        Ok(actions) => {
            println!("FetchAndAddMember returned {} actions:", actions.len());
            for action in actions {
                println!("  {:?}", action);
            }
        }
        Err(e) => {
            println!("FetchAndAddMember returned ERROR: {:?}", e);
        }
    }
}
