//! End-to-end test for multi-client messaging.
//!
//! Tests the complete flow: room creation, member addition, and bidirectional
//! message exchange with content verification.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use lockframe_client::{Client, ClientAction, ClientEvent, ClientIdentity};
use lockframe_core::env::Environment;
use lockframe_proto::{Frame, Opcode, Payload};
use lockframe_server::{DriverConfig, MemoryStorage, ServerAction, ServerDriver, ServerEvent};

#[derive(Clone)]
struct TestEnv;

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

struct TestHarness {
    server: ServerDriver<TestEnv, MemoryStorage>,
    room_members: HashMap<u128, HashSet<u64>>,
}

impl TestHarness {
    fn new() -> Self {
        let env = TestEnv;
        let storage = MemoryStorage::new();
        Self {
            server: ServerDriver::new(env, storage, DriverConfig::default()),
            room_members: HashMap::new(),
        }
    }

    fn connect(&mut self, session_id: u64, user_id: u64) {
        self.server
            .process_event(ServerEvent::ConnectionAccepted { session_id })
            .expect("connection accepted");

        let hello = Payload::Hello(lockframe_proto::payloads::session::Hello {
            version: 1,
            capabilities: vec![],
            sender_id: Some(user_id),
            auth_token: None,
        })
        .into_frame(lockframe_proto::FrameHeader::new(Opcode::Hello))
        .expect("hello frame");

        self.server
            .process_event(ServerEvent::FrameReceived { session_id, frame: hello })
            .expect("hello processed");
    }

    fn add_to_room(&mut self, room_id: u128, session_id: u64) {
        self.room_members.entry(room_id).or_default().insert(session_id);
    }

    fn route_frames(&mut self, session_id: u64, frames: Vec<Frame>) -> Vec<ServerAction> {
        let mut actions = Vec::new();
        for frame in frames {
            let result = self
                .server
                .process_event(ServerEvent::FrameReceived { session_id, frame })
                .expect("frame processed");
            actions.extend(result);
        }
        actions
    }

    fn frames_for_session(&self, actions: &[ServerAction], target: u64) -> Vec<Frame> {
        actions
            .iter()
            .filter_map(|action| match action {
                ServerAction::SendToSession { session_id, frame } if *session_id == target => {
                    Some(frame.clone())
                },
                ServerAction::BroadcastToRoom { room_id, frame, exclude_session } => {
                    let dominated = self.room_members.get(room_id);
                    let is_member = dominated.is_some_and(|m| m.contains(&target));
                    let is_excluded = *exclude_session == Some(target);
                    if is_member && !is_excluded { Some(frame.clone()) } else { None }
                },
                _ => None,
            })
            .collect()
    }
}

fn create_client(sender_id: u64) -> Client<TestEnv> {
    Client::new(TestEnv, ClientIdentity { sender_id })
}

fn extract_sends(actions: &[ClientAction]) -> Vec<Frame> {
    actions
        .iter()
        .filter_map(|a| match a {
            ClientAction::Send(f) => Some(f.clone()),
            _ => None,
        })
        .collect()
}

struct DeliveredMessage {
    sender_id: u64,
    plaintext: Vec<u8>,
}

fn find_delivered_message(actions: &[ClientAction]) -> Option<DeliveredMessage> {
    actions.iter().find_map(|a| match a {
        ClientAction::DeliverMessage { sender_id, plaintext, .. } => {
            Some(DeliveredMessage { sender_id: *sender_id, plaintext: plaintext.clone() })
        },
        _ => None,
    })
}

#[test]
fn test_bidirectional_messaging() {
    let mut harness = TestHarness::new();
    let mut alice = create_client(1000);
    let mut bob = create_client(2000);

    const ALICE_SESSION: u64 = 101;
    const BOB_SESSION: u64 = 102;
    const ROOM_ID: u128 = 1;

    // Setup: connect and authenticate
    harness.connect(ALICE_SESSION, 1000);
    harness.connect(BOB_SESSION, 2000);

    // Alice creates room
    let actions = alice.handle(ClientEvent::CreateRoom { room_id: ROOM_ID }).unwrap();
    harness.route_frames(ALICE_SESSION, extract_sends(&actions));
    harness.add_to_room(ROOM_ID, ALICE_SESSION);

    // Bob publishes KeyPackage
    let actions = bob.handle(ClientEvent::PublishKeyPackage).unwrap();
    harness.route_frames(BOB_SESSION, extract_sends(&actions));

    // Alice fetches Bob's KeyPackage and adds him
    let actions =
        alice.handle(ClientEvent::FetchAndAddMember { room_id: ROOM_ID, user_id: 2000 }).unwrap();
    let server_actions = harness.route_frames(ALICE_SESSION, extract_sends(&actions));

    // Alice processes KeyPackage response, generates Commit + Welcome
    let response_frames = harness.frames_for_session(&server_actions, ALICE_SESSION);
    assert!(!response_frames.is_empty(), "Alice should receive KeyPackage response");

    for frame in response_frames {
        let actions = alice.handle(ClientEvent::FrameReceived(frame)).unwrap();
        let sends = extract_sends(&actions);

        if sends.is_empty() {
            continue;
        }

        // Route Commit + Welcome to server
        let server_actions = harness.route_frames(ALICE_SESSION, sends);
        harness.add_to_room(ROOM_ID, BOB_SESSION);

        // Alice processes her own Commit broadcast
        for frame in harness.frames_for_session(&server_actions, ALICE_SESSION) {
            if frame.header.opcode_enum() == Some(Opcode::Commit) {
                alice.handle(ClientEvent::FrameReceived(frame)).unwrap();
            }
        }

        // Bob processes Welcome
        let welcome_frames: Vec<_> = harness
            .frames_for_session(&server_actions, BOB_SESSION)
            .into_iter()
            .filter(|f| f.header.opcode_enum() == Some(Opcode::Welcome))
            .collect();

        assert_eq!(welcome_frames.len(), 1, "Bob should receive exactly one Welcome");

        let actions = bob.handle(ClientEvent::FrameReceived(welcome_frames[0].clone())).unwrap();
        assert!(
            actions.iter().any(|a| matches!(a, ClientAction::PersistRoom(_))),
            "Bob should generate PersistRoom after Welcome"
        );
    }

    // Verify both clients are members at the same epoch
    assert!(alice.is_member(ROOM_ID), "Alice should be room member");
    assert!(bob.is_member(ROOM_ID), "Bob should be room member");
    assert_eq!(alice.epoch(ROOM_ID), bob.epoch(ROOM_ID), "Alice and Bob should be at same epoch");

    // Test 1: Alice sends message to Bob
    let alice_message = b"Hello from Alice!";
    let actions = alice
        .handle(ClientEvent::SendMessage { room_id: ROOM_ID, plaintext: alice_message.to_vec() })
        .unwrap();

    let server_actions = harness.route_frames(ALICE_SESSION, extract_sends(&actions));
    let bob_frames = harness.frames_for_session(&server_actions, BOB_SESSION);

    assert!(!bob_frames.is_empty(), "Bob should receive Alice's message frame");

    let mut bob_received = false;
    for frame in bob_frames {
        let actions = bob.handle(ClientEvent::FrameReceived(frame)).unwrap();
        if let Some(msg) = find_delivered_message(&actions) {
            assert_eq!(msg.plaintext, alice_message, "Bob should receive correct message content");
            assert_eq!(msg.sender_id, 1000, "Message should be from Alice");
            bob_received = true;
        }
    }
    assert!(bob_received, "Bob must receive and decrypt Alice's message");

    // Test 2: Bob sends message to Alice
    let bob_message = b"Hello from Bob!";
    let actions = bob
        .handle(ClientEvent::SendMessage { room_id: ROOM_ID, plaintext: bob_message.to_vec() })
        .unwrap();

    let server_actions = harness.route_frames(BOB_SESSION, extract_sends(&actions));
    let alice_frames = harness.frames_for_session(&server_actions, ALICE_SESSION);

    assert!(!alice_frames.is_empty(), "Alice should receive Bob's message frame");

    let mut alice_received = false;
    for frame in alice_frames {
        let actions = alice.handle(ClientEvent::FrameReceived(frame)).unwrap();
        if let Some(msg) = find_delivered_message(&actions) {
            assert_eq!(msg.plaintext, bob_message, "Alice should receive correct message content");
            assert_eq!(msg.sender_id, 2000, "Message should be from Bob");
            alice_received = true;
        }
    }
    assert!(alice_received, "Alice must receive and decrypt Bob's message");
}
