//! Room Manager tests
//!
//! Tests for the routing-only server RoomManager.
//! The server does NOT participate in MLS - it just routes frames between clients.

use std::time::Duration;

use bytes::Bytes;
use lockframe_core::env::Environment;
use lockframe_proto::{Frame, FrameHeader, Opcode};
use lockframe_server::{MemoryStorage, RoomAction, RoomError, RoomManager, Storage};

// Test environment using system RNG (std::time::Instant)
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
        rand::thread_rng().fill_bytes(buffer);
    }
}

#[test]
fn room_manager_new_has_no_rooms() {
    let manager = RoomManager::new();
    assert!(!manager.has_room(0x1234));
}

#[test]
fn create_room_succeeds_for_new_room() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    let result = manager.create_room(room_id, creator, &env);
    assert!(result.is_ok());
    assert!(manager.has_room(room_id));
}

#[test]
fn create_room_rejects_duplicate() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    // First creation succeeds
    manager.create_room(room_id, creator, &env).unwrap();

    // Second creation fails
    let result = manager.create_room(room_id, creator, &env);
    assert!(matches!(result, Err(RoomError::RoomAlreadyExists(_))));
}

#[test]
fn create_room_stores_metadata() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    manager.create_room(room_id, creator, &env).unwrap();

    // Metadata should be stored (we'll verify this when we add getter methods)
    assert!(manager.has_room(room_id));
}

#[test]
fn create_multiple_rooms() {
    let env = TestEnv;
    let mut manager = RoomManager::new();

    let room1 = 0x1111_1111_1111_1111_1111_1111_1111_1111;
    let room2 = 0x2222_2222_2222_2222_2222_2222_2222_2222;
    let room3 = 0x3333_3333_3333_3333_3333_3333_3333_3333;

    manager.create_room(room1, 1, &env).unwrap();
    manager.create_room(room2, 2, &env).unwrap();
    manager.create_room(room3, 3, &env).unwrap();

    assert!(manager.has_room(room1));
    assert!(manager.has_room(room2));
    assert!(manager.has_room(room3));
}

#[test]
fn process_frame_rejects_unknown_room() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let storage = MemoryStorage::new();

    // Create a frame for a room that doesn't exist
    let mut header = FrameHeader::new(Opcode::AppMessage);
    header.set_room_id(0x9999_9999_9999_9999_9999_9999_9999_9999);
    header.set_sender_id(42);
    header.set_epoch(0);
    let frame = Frame::new(header, Bytes::new());

    let result = manager.process_frame(frame, &env, &storage);
    assert!(matches!(result, Err(RoomError::RoomNotFound(_))));
}

#[test]
fn process_frame_succeeds_for_valid_frame() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let storage = MemoryStorage::new();

    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    // Create the room first
    manager.create_room(room_id, creator, &env).unwrap();

    // Create a valid frame
    let mut header = FrameHeader::new(Opcode::AppMessage);
    header.set_room_id(room_id);
    header.set_sender_id(creator);
    header.set_epoch(0);
    let frame = Frame::new(header, Bytes::new());

    let result = manager.process_frame(frame, &env, &storage);
    if let Err(ref e) = result {
        panic!("process_frame failed: {:?}", e);
    }
    assert!(result.is_ok());

    let actions = result.unwrap();
    // Should have actions (AcceptFrame becomes PersistFrame, StoreFrame becomes
    // PersistFrame, BroadcastToRoom becomes Broadcast) Sequencer returns 3
    // actions: AcceptFrame, StoreFrame, BroadcastToRoom
    assert!(!actions.is_empty());
    assert_eq!(actions.len(), 3);
}

#[test]
fn process_frame_returns_correct_action_types() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let storage = MemoryStorage::new();

    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    // Create the room first
    manager.create_room(room_id, creator, &env).unwrap();

    // Create a valid frame
    let mut header = FrameHeader::new(Opcode::AppMessage);
    header.set_room_id(room_id);
    header.set_sender_id(creator);
    header.set_epoch(0);
    let frame = Frame::new(header, Bytes::from("test message"));

    let result = manager.process_frame(frame, &env, &storage);
    assert!(result.is_ok());

    let actions = result.unwrap();

    // Verify we have the right action types
    // First two should be PersistFrame (from AcceptFrame and StoreFrame)
    assert!(matches!(actions[0], RoomAction::PersistFrame { .. }));
    assert!(matches!(actions[1], RoomAction::PersistFrame { .. }));

    // Last should be Broadcast (from BroadcastToRoom)
    assert!(matches!(actions[2], RoomAction::Broadcast { .. }));
}

/// Test that the server routes frames without MLS validation.
/// Server is routing-only - clients own the MLS state.
#[test]
fn process_frame_routes_any_epoch() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let storage = MemoryStorage::new();

    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    // Create room
    manager.create_room(room_id, creator, &env).unwrap();

    // Server is routing-only, should accept any epoch
    for epoch in [0, 1, 5, 100] {
        let mut header = FrameHeader::new(Opcode::AppMessage);
        header.set_room_id(room_id);
        header.set_sender_id(creator);
        header.set_epoch(epoch);
        let frame = Frame::new(header, Bytes::from(format!("msg at epoch {epoch}")));

        let result = manager.process_frame(frame, &env, &storage);
        assert!(result.is_ok(), "Server should route frame at epoch {epoch}");
    }
}

/// Test that handle_sync_request loads frames from storage and returns them.
#[test]
fn handle_sync_request_returns_stored_frames() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let storage = MemoryStorage::new();

    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;
    let requester = 100;

    // Create room
    manager.create_room(room_id, creator, &env).unwrap();

    // Store some frames directly in storage
    for i in 0..5 {
        let mut header = FrameHeader::new(Opcode::AppMessage);
        header.set_room_id(room_id);
        header.set_sender_id(creator);
        header.set_log_index(i);
        header.set_epoch(0);
        let frame = Frame::new(header, Bytes::from(format!("message {i}")));
        storage.store_frame(room_id, i, &frame).unwrap();
    }

    // Request sync from index 0
    let result = manager.handle_sync_request(room_id, requester, 0, 10, &env, &storage);
    assert!(result.is_ok());

    let action = result.unwrap();
    match action {
        RoomAction::SendSyncResponse { sender_id, room_id: rid, frames, has_more, .. } => {
            assert_eq!(sender_id, requester);
            assert_eq!(rid, room_id);
            assert_eq!(frames.len(), 5);
            assert!(!has_more);
        },
        _ => panic!("Expected SendSyncResponse action"),
    }
}

/// Test that handle_sync_request respects limit and sets has_more.
#[test]
fn handle_sync_request_paginates_with_limit() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let storage = MemoryStorage::new();

    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    // Create room
    manager.create_room(room_id, creator, &env).unwrap();

    // Store 10 frames
    for i in 0..10 {
        let mut header = FrameHeader::new(Opcode::AppMessage);
        header.set_room_id(room_id);
        header.set_sender_id(creator);
        header.set_log_index(i);
        header.set_epoch(0);
        let frame = Frame::new(header, Bytes::from(format!("message {i}")));
        storage.store_frame(room_id, i, &frame).unwrap();
    }

    // Request sync with limit of 3
    let result = manager.handle_sync_request(room_id, 100, 0, 3, &env, &storage);
    assert!(result.is_ok());

    let action = result.unwrap();
    match action {
        RoomAction::SendSyncResponse { frames, has_more, .. } => {
            assert_eq!(frames.len(), 3);
            assert!(has_more, "Should indicate more frames available");
        },
        _ => panic!("Expected SendSyncResponse action"),
    }

    // Request next batch starting from index 3
    let result = manager.handle_sync_request(room_id, 100, 3, 3, &env, &storage);
    assert!(result.is_ok());

    let action = result.unwrap();
    match action {
        RoomAction::SendSyncResponse { frames, has_more, .. } => {
            assert_eq!(frames.len(), 3);
            assert!(has_more, "Should still indicate more frames available");
        },
        _ => panic!("Expected SendSyncResponse action"),
    }
}

/// Test that handle_sync_request returns error for unknown room.
#[test]
fn handle_sync_request_unknown_room_fails() {
    let env = TestEnv;
    let manager = RoomManager::new();
    let storage = MemoryStorage::new();

    let result = manager.handle_sync_request(
        0x9999_9999_9999_9999_9999_9999_9999_9999,
        100,
        0,
        10,
        &env,
        &storage,
    );

    assert!(matches!(result, Err(RoomError::RoomNotFound(_))));
}

/// Test that server routes Commit frames like any other frame.
/// Server doesn't process MLS commits - it just routes them.
#[test]
fn process_commit_routes_without_mls_validation() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let storage = MemoryStorage::new();

    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    // Create room
    manager.create_room(room_id, creator, &env).unwrap();

    // Create a Commit frame (server just routes it)
    let mut header = FrameHeader::new(Opcode::Commit);
    header.set_room_id(room_id);
    header.set_sender_id(creator);
    header.set_epoch(0);
    let frame = Frame::new(header, Bytes::from("commit payload"));

    let result = manager.process_frame(frame, &env, &storage);
    assert!(result.is_ok(), "Server should route Commit frame");

    let actions = result.unwrap();
    assert!(!actions.is_empty(), "Should produce routing actions");
}

/// Test that server routes Welcome frames to recipients.
#[test]
fn process_welcome_routes_without_mls_validation() {
    let env = TestEnv;
    let mut manager = RoomManager::new();
    let storage = MemoryStorage::new();

    let room_id = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdef;
    let creator = 42;

    // Create room
    manager.create_room(room_id, creator, &env).unwrap();

    // Create a Welcome frame (server just routes it)
    let mut header = FrameHeader::new(Opcode::Welcome);
    header.set_room_id(room_id);
    header.set_sender_id(creator);
    header.set_epoch(0);
    let frame = Frame::new(header, Bytes::from("welcome payload"));

    let result = manager.process_frame(frame, &env, &storage);
    assert!(result.is_ok(), "Server should route Welcome frame");
}
