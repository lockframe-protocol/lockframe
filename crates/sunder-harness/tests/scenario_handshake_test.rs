//! Scenario test for connection handshake using state machine.
//!
//! This test validates the complete handshake flow using the scenario
//! framework, which automatically executes the handshake between clients and
//! servers.

use sunder_core::connection::ConnectionState;
use sunder_harness::scenario::{Scenario, oracle};

#[test]
fn scenario_handshake_single_client_server() {
    let result = Scenario::new("single client-server handshake")
        .client("alice")
        .server("hub")
        .oracle(Box::new(|world| {
            // Verify both actors exist
            let alice = world.client("alice").ok_or("alice should exist")?;
            let hub = world.server("hub").ok_or("hub should exist")?;

            // Verify both are authenticated after handshake
            if alice.state() != ConnectionState::Authenticated {
                return Err(format!("alice should be Authenticated, got {:?}", alice.state()));
            }

            if hub.state() != ConnectionState::Authenticated {
                return Err(format!("hub should be Authenticated, got {:?}", hub.state()));
            }

            // Verify session IDs match
            let alice_session = alice.session_id().ok_or("alice should have session_id")?;
            let hub_session = hub.session_id().ok_or("hub should have session_id")?;

            if alice_session != hub_session {
                return Err(format!(
                    "session IDs should match: alice={:x}, hub={:x}",
                    alice_session, hub_session
                ));
            }

            // Verify frame counts
            if world.frames_sent("alice") != 1 {
                return Err(format!(
                    "alice should have sent 1 frame, got {}",
                    world.frames_sent("alice")
                ));
            }

            if world.frames_received("alice") != 1 {
                return Err(format!(
                    "alice should have received 1 frame, got {}",
                    world.frames_received("alice")
                ));
            }

            if world.frames_sent("hub") != 1 {
                return Err(format!(
                    "hub should have sent 1 frame, got {}",
                    world.frames_sent("hub")
                ));
            }

            if world.frames_received("hub") != 1 {
                return Err(format!(
                    "hub should have received 1 frame, got {}",
                    world.frames_received("hub")
                ));
            }

            Ok(())
        }))
        .run();

    assert!(result.is_ok(), "scenario failed: {:?}", result);
}

#[test]
fn scenario_handshake_validates_frame_counts() {
    let result = Scenario::new("frame count validation")
        .client("alice")
        .server("hub")
        .oracle(Box::new(|world| {
            let alice = world.client("alice").ok_or("alice should exist")?;
            let hub = world.server("hub").ok_or("hub should exist")?;

            // Both should be authenticated
            assert_eq!(alice.state(), ConnectionState::Authenticated);
            assert_eq!(hub.state(), ConnectionState::Authenticated);

            // Verify exact frame counts for handshake
            // Client: sends 1 Hello, receives 1 HelloReply
            assert_eq!(world.frames_sent("alice"), 1, "alice should send 1 frame (Hello)");
            assert_eq!(
                world.frames_received("alice"),
                1,
                "alice should receive 1 frame (HelloReply)"
            );

            // Server: receives 1 Hello, sends 1 HelloReply
            assert_eq!(world.frames_sent("hub"), 1, "hub should send 1 frame (HelloReply)");
            assert_eq!(world.frames_received("hub"), 1, "hub should receive 1 frame (Hello)");

            Ok(())
        }))
        .run();

    assert!(result.is_ok(), "scenario failed: {:?}", result);
}

#[test]
fn scenario_handshake_use_oracle_helpers() {
    let result = Scenario::new("using oracle helper functions")
        .client("alice")
        .server("hub")
        .oracle(oracle::all_of(vec![oracle::all_authenticated(), oracle::session_ids_match()]))
        .run();

    assert!(result.is_ok(), "scenario failed: {:?}", result);
}
