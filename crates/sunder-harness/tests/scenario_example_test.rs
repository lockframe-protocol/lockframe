//! Example scenario tests demonstrating oracle patterns.

use sunder_harness::scenario::{Scenario, oracle};

#[test]
fn scenario_framework_simple() {
    let result = Scenario::new()
        .oracle(Box::new(|world| {
            let client = world.client();
            let server = world.server();

            // Custom verification logic
            if client.session_id() != server.session_id() {
                return Err("session IDs must match".to_string());
            }

            Ok(())
        }))
        .run();

    assert!(result.is_ok());
}

#[test]
fn scenario_framework_oracle_composition() {
    let result = Scenario::new()
        .oracle(oracle::all_of(vec![oracle::all_authenticated(), oracle::session_ids_match()]))
        .run();

    assert!(result.is_ok());
}
