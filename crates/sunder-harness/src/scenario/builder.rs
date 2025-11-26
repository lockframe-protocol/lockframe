//! Scenario builder API.
//!
//! Provides a declarative API for constructing scenario tests that enforce
//! the Oracle Pattern.

use std::time::Instant;

use sunder_core::connection::{Connection, ConnectionAction, ConnectionConfig};

use crate::scenario::{OracleFn, World};

/// Scenario builder.
///
/// Construct a scenario by adding clients, servers, and network operations.
/// Must call `.oracle()` to get a RunnableScenario that can be executed.
pub struct Scenario {
    name: String,
    clients: Vec<(String, ConnectionConfig)>,
    servers: Vec<(String, ConnectionConfig)>,
}

impl Scenario {
    /// Create a new scenario with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), clients: Vec::new(), servers: Vec::new() }
    }

    /// Add a client actor to the scenario.
    ///
    /// The client will be created with default configuration.
    pub fn client(mut self, name: impl Into<String>) -> Self {
        self.clients.push((name.into(), ConnectionConfig::default()));
        self
    }

    /// Add a client actor with custom configuration.
    pub fn client_with_config(mut self, name: impl Into<String>, config: ConnectionConfig) -> Self {
        self.clients.push((name.into(), config));
        self
    }

    /// Add a server actor to the scenario.
    ///
    /// The server will be created with default configuration.
    pub fn server(mut self, name: impl Into<String>) -> Self {
        self.servers.push((name.into(), ConnectionConfig::default()));
        self
    }

    /// Add a server actor with custom configuration.
    pub fn server_with_config(mut self, name: impl Into<String>, config: ConnectionConfig) -> Self {
        self.servers.push((name.into(), config));
        self
    }

    /// Set the oracle function and return a runnable scenario.
    ///
    /// The oracle is mandatory - you cannot run a scenario without
    /// verification.
    pub fn oracle(self, oracle: OracleFn) -> RunnableScenario {
        RunnableScenario { scenario: self, oracle }
    }
}

/// A scenario with an oracle function that can be executed.
pub struct RunnableScenario {
    scenario: Scenario,
    oracle: OracleFn,
}

impl RunnableScenario {
    /// Execute the scenario.
    ///
    /// Performs a complete handshake between all clients and all servers,
    /// then runs the oracle to verify the final state.
    ///
    /// For each client-server pair:
    /// 1. Client sends Hello
    /// 2. Server handles Hello and sends HelloReply
    /// 3. Client handles HelloReply and transitions to Authenticated
    ///
    /// After all handshakes complete, the oracle is invoked to verify
    /// global consistency.
    pub fn run(self) -> Result<(), String> {
        let mut world = World::new();
        let now = Instant::now();

        for (name, config) in self.scenario.clients {
            let connection = Connection::new(now, config);
            world.add_client(name, connection);
        }

        for (name, config) in self.scenario.servers {
            let mut connection = Connection::new(now, config);
            let unique_id = 0x1000_0000_0000_0000 + (world.actor_names().len() as u64);
            connection.set_session_id(unique_id);
            world.add_server(name, connection);
        }

        let client_names: Vec<String> = world
            .actor_names()
            .iter()
            .filter(|name| world.client(name).is_some())
            .cloned()
            .collect();

        let server_names: Vec<String> = world
            .actor_names()
            .iter()
            .filter(|name| world.server(name).is_some())
            .cloned()
            .collect();

        // Note: In a real distributed system, each client-server connection is
        // independent. For now, we only support 1:1 handshakes (one client per
        // server or vice versa). Multi-client scenarios will need network
        // simulation (turmoil) to properly model separate connection instances.
        // For now, we only support single client + single server scenarios
        if client_names.len() != 1 || server_names.len() != 1 {
            return Err(format!(
                "Scenario '{}': Current implementation only supports 1 client and 1 server (got {} clients, {} servers). \
                 Multi-actor scenarios require turmoil integration.",
                self.scenario.name,
                client_names.len(),
                server_names.len()
            ));
        }

        for client_name in &client_names {
            for server_name in &server_names {
                let hello_frame = {
                    let client = world.client_mut(client_name).ok_or_else(|| {
                        format!(
                            "Scenario '{}': client {} not found",
                            self.scenario.name, client_name
                        )
                    })?;

                    let actions = client.send_hello(now).map_err(|e| {
                        format!(
                            "Scenario '{}': client {} send_hello failed: {}",
                            self.scenario.name, client_name, e
                        )
                    })?;

                    match actions.as_slice() {
                        [ConnectionAction::SendFrame(frame)] => frame.clone(),
                        _ => {
                            return Err(format!(
                                "Scenario '{}': client {} send_hello returned unexpected actions",
                                self.scenario.name, client_name
                            ));
                        },
                    }
                };

                world.record_frame_sent(client_name);
                world.record_frame_received(server_name);

                let hello_reply_frame = {
                    let server = world.server_mut(server_name).ok_or_else(|| {
                        format!(
                            "Scenario '{}': server {} not found",
                            self.scenario.name, server_name
                        )
                    })?;

                    let actions = server.handle_frame(&hello_frame, now).map_err(|e| {
                        format!(
                            "Scenario '{}': server {} handle_frame(Hello) failed: {}",
                            self.scenario.name, server_name, e
                        )
                    })?;

                    match actions.as_slice() {
                        [ConnectionAction::SendFrame(frame)] => frame.clone(),
                        _ => {
                            return Err(format!(
                                "Scenario '{}': server {} handle_frame(Hello) returned unexpected actions",
                                self.scenario.name, server_name
                            ));
                        },
                    }
                };

                world.record_frame_sent(server_name);
                world.record_frame_received(client_name);

                {
                    let client = world.client_mut(client_name).ok_or_else(|| {
                        format!(
                            "Scenario '{}': client {} not found",
                            self.scenario.name, client_name
                        )
                    })?;

                    let actions = client.handle_frame(&hello_reply_frame, now).map_err(|e| {
                        format!(
                            "Scenario '{}': client {} handle_frame(HelloReply) failed: {}",
                            self.scenario.name, client_name, e
                        )
                    })?;

                    if !actions.is_empty() {
                        return Err(format!(
                            "Scenario '{}': client {} handle_frame(HelloReply) returned unexpected actions",
                            self.scenario.name, client_name
                        ));
                    }
                }
            }
        }

        (self.oracle)(&world)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_requires_oracle() {
        // This should compile - oracle provided
        let _scenario = Scenario::new("test").client("alice").oracle(Box::new(|_world| Ok(())));

        // This should NOT compile - no oracle
        // let scenario = Scenario::new("test").client("alice");
        // scenario.run(); // ERROR: no method `run` on type `Scenario`
    }

    #[test]
    fn scenario_creates_actors() {
        let scenario =
            Scenario::new("test").client("alice").server("hub").oracle(Box::new(|world| {
                assert!(world.client("alice").is_some());
                assert!(world.server("hub").is_some());
                Ok(())
            }));

        scenario.run().expect("scenario should succeed");
    }
}
