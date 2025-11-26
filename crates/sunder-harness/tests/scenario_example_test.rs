//! Example scenario test demonstrating the scenario framework.

use sunder_harness::scenario::{Scenario, oracle};

#[test]
fn scenario_framework_basic_example() {
    // This is a minimal example showing the scenario API
    // The scenario creates a client and server, executes handshake, then verifies
    // final state
    let result = Scenario::new("basic example")
        .client("alice")
        .server("hub")
        .oracle(Box::new(|world| {
            // Verify actors were created
            assert!(world.client("alice").is_some(), "alice client should exist");
            assert!(world.server("hub").is_some(), "hub server should exist");

            // Verify they completed handshake and are now Authenticated
            let alice = world.client("alice").unwrap();
            let hub = world.server("hub").unwrap();

            assert_eq!(
                alice.state(),
                sunder_core::connection::ConnectionState::Authenticated,
                "alice should be Authenticated after handshake"
            );
            assert_eq!(
                hub.state(),
                sunder_core::connection::ConnectionState::Authenticated,
                "hub should be Authenticated after handshake"
            );

            // Verify session IDs match
            assert_eq!(alice.session_id(), hub.session_id(), "session IDs should match");

            Ok(())
        }))
        .run();

    assert!(result.is_ok(), "scenario should succeed: {:?}", result);
}

#[test]
fn scenario_framework_oracle_helpers() {
    // Example using oracle helper functions
    let result = Scenario::new("oracle helpers")
        .client("client1")
        .server("server1")
        .oracle(oracle::all_authenticated())
        .run();

    assert!(result.is_ok(), "scenario should succeed: {:?}", result);
}

#[test]
fn scenario_framework_oracle_composition() {
    // Example using oracle::all_of to compose multiple verifications
    let result = Scenario::new("oracle composition")
        .client("alice")
        .server("hub")
        .oracle(oracle::all_of(vec![oracle::all_authenticated(), oracle::session_ids_match()]))
        .run();

    assert!(result.is_ok(), "scenario should succeed: {:?}", result);
}
