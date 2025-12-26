//! Model-based property tests.
//!
//! These tests generate random operation sequences and verify that the real
//! implementation behaves identically to the reference model.
//!
//! # Architecture
//!
//! ```text
//! proptest generates: Vec<Operation>
//!                          │
//!           ┌──────────────┼──────────────┐
//!           ▼              ▼              ▼
//!      ModelWorld    RealWorld      Compare
//!      (reference)   (turmoil)      Results
//! ```

use std::collections::HashMap;

use lockframe_client::{Client, ClientIdentity};
use lockframe_harness::{
    ClientId, ModelRoomId, ModelWorld, Operation, OperationError, OperationResult, SimEnv,
    SmallMessage,
};
use proptest::prelude::*;

/// Real system wrapper that mirrors ModelWorld's interface.
struct RealWorld {
    clients: Vec<Client<SimEnv>>,
    /// Environment for time/randomness (kept for future AdvanceTime support).
    #[allow(dead_code)]
    env: SimEnv,
    /// Track room membership (real client tracks internally but we need to map
    /// room IDs)
    room_membership: HashMap<(ClientId, ModelRoomId), bool>,
}

impl RealWorld {
    fn new(num_clients: usize, seed: u64) -> Self {
        let env = SimEnv::with_seed(seed);
        let clients = (0..num_clients)
            .map(|i| {
                let identity = ClientIdentity::new(i as u64 + 1); // sender_id starts at 1
                Client::new(env.clone(), identity)
            })
            .collect();

        Self { clients, env, room_membership: HashMap::new() }
    }

    fn apply(&mut self, op: &Operation) -> OperationResult {
        match op {
            Operation::CreateRoom { client_id, room_id } => {
                self.apply_create_room(*client_id, *room_id)
            },
            Operation::SendMessage { client_id, room_id, content } => {
                self.apply_send_message(*client_id, *room_id, content)
            },
            Operation::LeaveRoom { client_id, room_id } => {
                self.apply_leave_room(*client_id, *room_id)
            },
            Operation::AddMember { inviter_id, invitee_id, room_id } => {
                self.apply_add_member(*inviter_id, *invitee_id, *room_id)
            },
            Operation::RemoveMember { remover_id, target_id, room_id } => {
                self.apply_remove_member(*remover_id, *target_id, *room_id)
            },
            Operation::AdvanceTime { .. } | Operation::DeliverPending => OperationResult::Ok,
        }
    }

    fn apply_add_member(
        &mut self,
        inviter_id: ClientId,
        invitee_id: ClientId,
        room_id: ModelRoomId,
    ) -> OperationResult {
        // Validate both clients exist
        if inviter_id as usize >= self.clients.len() {
            return OperationResult::Error(OperationError::InvalidClient);
        }
        if invitee_id as usize >= self.clients.len() {
            return OperationResult::Error(OperationError::InvalidClient);
        }

        // Check inviter is member
        if !self.room_membership.get(&(inviter_id, room_id)).copied().unwrap_or(false) {
            return OperationResult::Error(OperationError::NotMember);
        }

        // Check invitee is NOT already a member
        if self.room_membership.get(&(invitee_id, room_id)).copied().unwrap_or(false) {
            return OperationResult::Error(OperationError::AlreadyMember);
        }

        // In a real implementation, this would involve MLS key packages and commits.
        // For now, we just track membership.
        self.room_membership.insert((invitee_id, room_id), true);
        OperationResult::Ok
    }

    fn apply_remove_member(
        &mut self,
        remover_id: ClientId,
        target_id: ClientId,
        room_id: ModelRoomId,
    ) -> OperationResult {
        // Validate both clients exist
        if remover_id as usize >= self.clients.len() {
            return OperationResult::Error(OperationError::InvalidClient);
        }
        if target_id as usize >= self.clients.len() {
            return OperationResult::Error(OperationError::InvalidClient);
        }

        // Cannot remove self
        if remover_id == target_id {
            return OperationResult::Error(OperationError::CannotRemoveSelf);
        }

        // Check remover is member
        if !self.room_membership.get(&(remover_id, room_id)).copied().unwrap_or(false) {
            return OperationResult::Error(OperationError::NotMember);
        }

        // Check target is member
        if !self.room_membership.get(&(target_id, room_id)).copied().unwrap_or(false) {
            return OperationResult::Error(OperationError::NotMember);
        }

        // In a real implementation, this would involve MLS proposals and commits.
        self.room_membership.insert((target_id, room_id), false);
        OperationResult::Ok
    }

    fn apply_create_room(&mut self, client_id: ClientId, room_id: ModelRoomId) -> OperationResult {
        let client = match self.clients.get_mut(client_id as usize) {
            Some(c) => c,
            None => return OperationResult::Error(OperationError::InvalidClient),
        };

        // Map ModelRoomId (u8) to real RoomId (u128)
        let real_room_id = room_id as u128 + 1; // Avoid room_id 0

        // Check if already member
        if self.room_membership.get(&(client_id, room_id)).copied().unwrap_or(false) {
            return OperationResult::Error(OperationError::RoomAlreadyExists);
        }

        use lockframe_client::ClientEvent;
        let result = client.handle(ClientEvent::CreateRoom { room_id: real_room_id });

        match result {
            Ok(_actions) => {
                self.room_membership.insert((client_id, room_id), true);
                OperationResult::Ok
            },
            Err(_) => OperationResult::Error(OperationError::RoomAlreadyExists),
        }
    }

    fn apply_send_message(
        &mut self,
        client_id: ClientId,
        room_id: ModelRoomId,
        content: &SmallMessage,
    ) -> OperationResult {
        let client = match self.clients.get_mut(client_id as usize) {
            Some(c) => c,
            None => return OperationResult::Error(OperationError::InvalidClient),
        };

        // Check membership
        if !self.room_membership.get(&(client_id, room_id)).copied().unwrap_or(false) {
            return OperationResult::Error(OperationError::NotMember);
        }

        let real_room_id = room_id as u128 + 1;
        let plaintext = content.to_bytes();

        use lockframe_client::ClientEvent;
        let result = client.handle(ClientEvent::SendMessage { room_id: real_room_id, plaintext });

        match result {
            Ok(_actions) => OperationResult::Ok,
            Err(_) => OperationResult::Error(OperationError::NotMember),
        }
    }

    fn apply_leave_room(&mut self, client_id: ClientId, room_id: ModelRoomId) -> OperationResult {
        let client = match self.clients.get_mut(client_id as usize) {
            Some(c) => c,
            None => return OperationResult::Error(OperationError::InvalidClient),
        };

        // Check membership
        if !self.room_membership.get(&(client_id, room_id)).copied().unwrap_or(false) {
            return OperationResult::Error(OperationError::NotMember);
        }

        let real_room_id = room_id as u128 + 1;

        use lockframe_client::ClientEvent;
        let result = client.handle(ClientEvent::LeaveRoom { room_id: real_room_id });

        match result {
            Ok(_actions) => {
                self.room_membership.insert((client_id, room_id), false);
                OperationResult::Ok
            },
            Err(_) => OperationResult::Error(OperationError::NotMember),
        }
    }
}

/// Strategy for generating SmallMessage.
fn small_message_strategy() -> impl Strategy<Value = SmallMessage> {
    (any::<u8>(), any::<u8>()).prop_map(|(seed, size_class)| SmallMessage { seed, size_class })
}

/// Strategy for generating operations with valid client IDs.
fn operation_strategy(num_clients: usize) -> impl Strategy<Value = Operation> {
    let client_id = 0..num_clients as u8;
    let room_id = any::<ModelRoomId>();
    let content = small_message_strategy();
    let millis = any::<u16>();

    prop_oneof![
        // Weight towards more interesting operations
        3 => (client_id.clone(), room_id.clone()).prop_map(|(c, r)| Operation::CreateRoom {
            client_id: c,
            room_id: r
        }),
        5 => (client_id.clone(), room_id.clone(), content).prop_map(|(c, r, content)| {
            Operation::SendMessage { client_id: c, room_id: r, content }
        }),
        1 => (client_id.clone(), room_id.clone()).prop_map(|(c, r)| Operation::LeaveRoom {
            client_id: c,
            room_id: r
        }),
        2 => (client_id.clone(), client_id.clone(), room_id.clone()).prop_map(|(i, e, r)| {
            Operation::AddMember { inviter_id: i, invitee_id: e, room_id: r }
        }),
        1 => (client_id.clone(), client_id.clone(), room_id.clone()).prop_map(|(r, t, room)| {
            Operation::RemoveMember { remover_id: r, target_id: t, room_id: room }
        }),
        1 => millis.prop_map(|m| Operation::AdvanceTime { millis: m }),
        1 => Just(Operation::DeliverPending),
    ]
}

proptest! {
    /// Verify that operation results match between model and real implementation.
    ///
    /// This is the core model-based test. It generates random operation sequences
    /// and asserts that both implementations return the same results.
    #[test]
    fn prop_model_matches_real(
        seed in any::<u64>(),
        num_clients in 2..5usize,
        ops in prop::collection::vec(operation_strategy(4), 0..50)
    ) {
        let mut model = ModelWorld::new(num_clients);
        let mut real = RealWorld::new(num_clients, seed);

        for (i, op) in ops.iter().enumerate() {
            // Clamp client_id to valid range
            let clamped_op = clamp_client_id(op.clone(), num_clients);

            let model_result = model.apply(&clamped_op);
            let real_result = real.apply(&clamped_op);

            // Results must match
            prop_assert_eq!(
                model_result.is_ok(),
                real_result.is_ok(),
                "Divergence at operation {}: {:?}\nModel: {:?}\nReal: {:?}",
                i, clamped_op, model_result, real_result
            );
        }
    }

    /// Verify model invariants hold after any operation sequence.
    #[test]
    fn prop_model_invariants(
        num_clients in 2..5usize,
        ops in prop::collection::vec(operation_strategy(4), 0..100)
    ) {
        let mut model = ModelWorld::new(num_clients);

        for op in ops {
            let clamped_op = clamp_client_id(op, num_clients);
            let _ = model.apply(&clamped_op);
        }

        // Invariant: Observable state is consistent
        let state = model.observable_state();

        // Invariant: Client room lists match server membership
        for (client_id, rooms) in state.client_rooms.iter().enumerate() {
            for room_id in rooms {
                prop_assert!(
                    model.server().is_member(*room_id, client_id as ClientId),
                    "Client {} claims membership in room {} but server disagrees",
                    client_id, room_id
                );
            }
        }

        // Invariant: All messages have sequential log indices
        for (room_id, messages) in &state.server_messages {
            for (i, msg) in messages.iter().enumerate() {
                prop_assert_eq!(
                    msg.log_index, i as u64,
                    "Room {} message {} has wrong log_index: expected {}, got {}",
                    room_id, i, i, msg.log_index
                );
            }
        }
    }

    /// Verify that room creation is idempotent (second create fails).
    #[test]
    fn prop_create_room_idempotent(
        client_id in 0..4u8,
        room_id in any::<ModelRoomId>()
    ) {
        let mut model = ModelWorld::new(4);

        // First create should succeed
        let first = model.apply(&Operation::CreateRoom { client_id, room_id });
        prop_assert!(first.is_ok(), "First create should succeed");

        // Second create should fail
        let second = model.apply(&Operation::CreateRoom { client_id, room_id });
        prop_assert!(second.is_err(), "Second create should fail");
    }

    /// Verify that messages are only accepted from members.
    #[test]
    fn prop_send_requires_membership(
        sender in 0..4u8,
        other in 0..4u8,
        room_id in any::<ModelRoomId>(),
        content in small_message_strategy()
    ) {
        prop_assume!(sender != other);

        let mut model = ModelWorld::new(4);

        // Sender creates room
        let _ = model.apply(&Operation::CreateRoom { client_id: sender, room_id });

        // Other client (not member) tries to send - should fail
        let result = model.apply(&Operation::SendMessage {
            client_id: other,
            room_id,
            content,
        });

        prop_assert!(result.is_err(), "Non-member send should fail");
    }

    /// Verify add member semantics.
    #[test]
    fn prop_add_member_semantics(
        creator in 0..4u8,
        invitee in 0..4u8,
        room_id in any::<ModelRoomId>()
    ) {
        prop_assume!(creator != invitee);

        let mut model = ModelWorld::new(4);

        // Create room
        let _ = model.apply(&Operation::CreateRoom { client_id: creator, room_id });

        // Add invitee
        let result = model.apply(&Operation::AddMember {
            inviter_id: creator,
            invitee_id: invitee,
            room_id,
        });
        prop_assert!(result.is_ok(), "Adding new member should succeed");

        // Invitee can now send
        let result = model.apply(&Operation::SendMessage {
            client_id: invitee,
            room_id,
            content: SmallMessage { seed: 1, size_class: 0 },
        });
        prop_assert!(result.is_ok(), "New member should be able to send");

        // Adding again should fail
        let result = model.apply(&Operation::AddMember {
            inviter_id: creator,
            invitee_id: invitee,
            room_id,
        });
        prop_assert!(result.is_err(), "Adding existing member should fail");
    }

    /// Verify remove member semantics.
    #[test]
    fn prop_remove_member_semantics(
        creator in 0..4u8,
        target in 0..4u8,
        room_id in any::<ModelRoomId>()
    ) {
        prop_assume!(creator != target);

        let mut model = ModelWorld::new(4);

        // Create room and add target
        let _ = model.apply(&Operation::CreateRoom { client_id: creator, room_id });
        let _ = model.apply(&Operation::AddMember {
            inviter_id: creator,
            invitee_id: target,
            room_id,
        });

        // Remove target
        let result = model.apply(&Operation::RemoveMember {
            remover_id: creator,
            target_id: target,
            room_id,
        });
        prop_assert!(result.is_ok(), "Removing member should succeed");

        // Target can no longer send
        let result = model.apply(&Operation::SendMessage {
            client_id: target,
            room_id,
            content: SmallMessage { seed: 1, size_class: 0 },
        });
        prop_assert!(result.is_err(), "Removed member should not be able to send");
    }

    /// Verify cannot remove self.
    #[test]
    fn prop_cannot_remove_self(
        client_id in 0..4u8,
        room_id in any::<ModelRoomId>()
    ) {
        let mut model = ModelWorld::new(4);

        // Create room
        let _ = model.apply(&Operation::CreateRoom { client_id, room_id });

        // Try to remove self
        let result = model.apply(&Operation::RemoveMember {
            remover_id: client_id,
            target_id: client_id,
            room_id,
        });
        prop_assert!(result.is_err(), "Should not be able to remove self");

        if let OperationResult::Error(e) = result {
            prop_assert_eq!(e, OperationError::CannotRemoveSelf);
        }
    }

    /// Verify error properties are consistent.
    #[test]
    fn prop_error_properties_consistent(
        seed in any::<u64>(),
        num_clients in 2..5usize,
        ops in prop::collection::vec(operation_strategy(4), 0..30)
    ) {
        let mut model = ModelWorld::new(num_clients);
        let mut real = RealWorld::new(num_clients, seed);

        for op in ops {
            let clamped_op = clamp_client_id(op, num_clients);

            let model_result = model.apply(&clamped_op);
            let real_result = real.apply(&clamped_op);

            // If both are errors, verify properties match
            match (&model_result, &real_result) {
                (OperationResult::Error(m_err), OperationResult::Error(r_err)) => {
                    let m_props = m_err.properties();
                    let r_props = r_err.properties();
                    prop_assert_eq!(
                        m_props, r_props,
                        "Error properties mismatch for {:?}: model={:?}, real={:?}",
                        clamped_op, m_err, r_err
                    );
                },
                _ => {},
            }
        }
    }
}

/// Clamp client_id to valid range for the given number of clients.
fn clamp_client_id(op: Operation, num_clients: usize) -> Operation {
    let clamp = |id: ClientId| id % num_clients as u8;
    match op {
        Operation::CreateRoom { client_id, room_id } => {
            Operation::CreateRoom { client_id: clamp(client_id), room_id }
        },
        Operation::SendMessage { client_id, room_id, content } => {
            Operation::SendMessage { client_id: clamp(client_id), room_id, content }
        },
        Operation::LeaveRoom { client_id, room_id } => {
            Operation::LeaveRoom { client_id: clamp(client_id), room_id }
        },
        Operation::AddMember { inviter_id, invitee_id, room_id } => Operation::AddMember {
            inviter_id: clamp(inviter_id),
            invitee_id: clamp(invitee_id),
            room_id,
        },
        Operation::RemoveMember { remover_id, target_id, room_id } => Operation::RemoveMember {
            remover_id: clamp(remover_id),
            target_id: clamp(target_id),
            room_id,
        },
        other => other,
    }
}

#[cfg(test)]
mod smoke_tests {
    use super::*;

    /// Basic smoke test for the model.
    #[test]
    fn model_basic_operations() {
        let mut model = ModelWorld::new(2);

        // Client 0 creates room
        let result = model.apply(&Operation::CreateRoom { client_id: 0, room_id: 1 });
        assert!(result.is_ok());

        // Client 0 sends message
        let result = model.apply(&Operation::SendMessage {
            client_id: 0,
            room_id: 1,
            content: SmallMessage { seed: 42, size_class: 1 },
        });
        assert!(result.is_ok());

        // Deliver pending messages
        model.apply(&Operation::DeliverPending);

        // Client 1 (not member) tries to send - should fail
        let result = model.apply(&Operation::SendMessage {
            client_id: 1,
            room_id: 1,
            content: SmallMessage { seed: 43, size_class: 1 },
        });
        assert!(result.is_err());

        // Client 0 leaves
        let result = model.apply(&Operation::LeaveRoom { client_id: 0, room_id: 1 });
        assert!(result.is_ok());

        // Client 0 tries to send after leaving - should fail
        let result = model.apply(&Operation::SendMessage {
            client_id: 0,
            room_id: 1,
            content: SmallMessage { seed: 44, size_class: 1 },
        });
        assert!(result.is_err());
    }

    /// Test membership operations.
    #[test]
    fn model_membership_operations() {
        let mut model = ModelWorld::new(3);

        // Client 0 creates room
        let result = model.apply(&Operation::CreateRoom { client_id: 0, room_id: 1 });
        assert!(result.is_ok());

        // Client 0 adds client 1
        let result =
            model.apply(&Operation::AddMember { inviter_id: 0, invitee_id: 1, room_id: 1 });
        assert!(result.is_ok());

        // Client 1 can now send
        let result = model.apply(&Operation::SendMessage {
            client_id: 1,
            room_id: 1,
            content: SmallMessage { seed: 1, size_class: 0 },
        });
        assert!(result.is_ok());

        // Client 0 removes client 1
        let result =
            model.apply(&Operation::RemoveMember { remover_id: 0, target_id: 1, room_id: 1 });
        assert!(result.is_ok());

        // Client 1 can no longer send
        let result = model.apply(&Operation::SendMessage {
            client_id: 1,
            room_id: 1,
            content: SmallMessage { seed: 2, size_class: 0 },
        });
        assert!(result.is_err());
    }

    /// Test pending delivery semantics.
    #[test]
    fn model_pending_delivery() {
        let mut model = ModelWorld::new(2);

        // Client 0 creates room
        model.apply(&Operation::CreateRoom { client_id: 0, room_id: 1 });

        // Client 0 adds client 1
        model.apply(&Operation::AddMember { inviter_id: 0, invitee_id: 1, room_id: 1 });

        // Client 0 sends message (not delivered yet)
        model.apply(&Operation::SendMessage {
            client_id: 0,
            room_id: 1,
            content: SmallMessage { seed: 42, size_class: 1 },
        });

        // Check pending count
        assert_eq!(model.server().pending_count(), 1);

        // Deliver
        model.apply(&Operation::DeliverPending);

        // Pending cleared
        assert_eq!(model.server().pending_count(), 0);

        // Client 1 should have the message
        let state = model.observable_state();
        assert!(!state.client_messages[1].is_empty());
    }

    /// Test error properties.
    #[test]
    fn error_properties() {
        // Fatal errors
        assert!(OperationError::InvalidClient.properties().is_fatal);
        assert!(OperationError::NotMember.properties().is_fatal);
        assert!(OperationError::CannotRemoveSelf.properties().is_fatal);

        // Non-fatal errors
        assert!(!OperationError::RoomNotFound.properties().is_fatal);
        assert!(!OperationError::RoomAlreadyExists.properties().is_fatal);
        assert!(!OperationError::AlreadyMember.properties().is_fatal);

        // Retryable errors
        assert!(OperationError::EpochMismatch { expected: 1, actual: 0 }.properties().is_retryable);
    }
}
