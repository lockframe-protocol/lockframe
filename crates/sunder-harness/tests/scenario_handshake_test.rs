//! Scenario test for connection handshake using state machine.
//!
//! This test validates the complete handshake flow using the scenario
//! framework, which automatically executes the handshake between client and
//! server.

use sunder_core::connection::ConnectionState;
use sunder_harness::scenario::{Scenario, oracle};

#[test]
fn scenario_handshake_single_client_server() {
    let result = Scenario::new()
        .oracle(Box::new(|world| {
            // Verify both are authenticated after handshake
            if world.client().state() != ConnectionState::Authenticated {
                return Err(format!(
                    "client should be Authenticated, got {:?}",
                    world.client().state()
                ));
            }

            if world.server().state() != ConnectionState::Authenticated {
                return Err(format!(
                    "server should be Authenticated, got {:?}",
                    world.server().state()
                ));
            }

            // Verify session IDs match
            let client_session =
                world.client().session_id().ok_or("client should have session_id")?;
            let server_session =
                world.server().session_id().ok_or("server should have session_id")?;

            if client_session != server_session {
                return Err(format!(
                    "session IDs should match: client={:x}, server={:x}",
                    client_session, server_session
                ));
            }

            // Verify frame counts
            if world.client_frames_sent() != 1 {
                return Err(format!(
                    "client should have sent 1 frame, got {}",
                    world.client_frames_sent()
                ));
            }

            if world.client_frames_received() != 1 {
                return Err(format!(
                    "client should have received 1 frame, got {}",
                    world.client_frames_received()
                ));
            }

            if world.server_frames_sent() != 1 {
                return Err(format!(
                    "server should have sent 1 frame, got {}",
                    world.server_frames_sent()
                ));
            }

            if world.server_frames_received() != 1 {
                return Err(format!(
                    "server should have received 1 frame, got {}",
                    world.server_frames_received()
                ));
            }

            Ok(())
        }))
        .run();

    assert!(result.is_ok(), "scenario failed: {:?}", result);
}

#[test]
fn scenario_handshake_validates_frame_counts() {
    let result = Scenario::new()
        .oracle(Box::new(|world| {
            // Both should be authenticated
            assert_eq!(world.client().state(), ConnectionState::Authenticated);
            assert_eq!(world.server().state(), ConnectionState::Authenticated);

            // Verify exact frame counts for handshake
            // Client: sends 1 Hello, receives 1 HelloReply
            assert_eq!(world.client_frames_sent(), 1, "client should send 1 frame (Hello)");
            assert_eq!(
                world.client_frames_received(),
                1,
                "client should receive 1 frame (HelloReply)"
            );

            // Server: receives 1 Hello, sends 1 HelloReply
            assert_eq!(world.server_frames_sent(), 1, "server should send 1 frame (HelloReply)");
            assert_eq!(world.server_frames_received(), 1, "server should receive 1 frame (Hello)");

            Ok(())
        }))
        .run();

    assert!(result.is_ok(), "scenario failed: {:?}", result);
}

#[test]
fn scenario_handshake_use_oracle_helpers() {
    let result = Scenario::new()
        .oracle(oracle::all_of(vec![oracle::all_authenticated(), oracle::session_ids_match()]))
        .run();

    assert!(result.is_ok(), "scenario failed: {:?}", result);
}
