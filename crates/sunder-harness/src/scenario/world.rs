//! World state for scenario execution.
//!
//! The World manages all actors (clients and servers) during scenario
//! execution, tracks metrics, and provides oracle verification helpers.

use std::collections::HashMap;

use sunder_core::connection::{Connection, ConnectionState};

/// Network events that occurred during scenario execution.
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkEvent {
    /// Network partition between two actors
    Partition { from: String, to: String },
    /// Network partition healed
    PartitionHealed { from: String, to: String },
    /// Packet loss injected
    PacketLoss { rate: f64 },
    /// Latency injected
    Latency { min_ms: u64, max_ms: u64 },
}

/// World state containing all actors and metrics.
pub struct World {
    clients: HashMap<String, Connection>,
    servers: HashMap<String, Connection>,
    frames_sent: HashMap<String, usize>,
    frames_received: HashMap<String, usize>,
    network_events: Vec<NetworkEvent>,
}

impl World {
    /// Create a new empty world.
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            servers: HashMap::new(),
            frames_sent: HashMap::new(),
            frames_received: HashMap::new(),
            network_events: Vec::new(),
        }
    }

    /// Add a client connection to the world.
    pub fn add_client(&mut self, name: String, connection: Connection) {
        self.clients.insert(name.clone(), connection);
        self.frames_sent.insert(name.clone(), 0);
        self.frames_received.insert(name, 0);
    }

    /// Add a server connection to the world.
    pub fn add_server(&mut self, name: String, connection: Connection) {
        self.servers.insert(name.clone(), connection);
        self.frames_sent.insert(name.clone(), 0);
        self.frames_received.insert(name, 0);
    }

    /// Get a client connection by name.
    pub fn client(&self, name: &str) -> Option<&Connection> {
        self.clients.get(name)
    }

    /// Get a server connection by name.
    pub fn server(&self, name: &str) -> Option<&Connection> {
        self.servers.get(name)
    }

    /// Get mutable client connection by name.
    pub fn client_mut(&mut self, name: &str) -> Option<&mut Connection> {
        self.clients.get_mut(name)
    }

    /// Get mutable server connection by name.
    pub fn server_mut(&mut self, name: &str) -> Option<&mut Connection> {
        self.servers.get_mut(name)
    }

    /// Record that a frame was sent by an actor.
    pub fn record_frame_sent(&mut self, actor: &str) {
        *self.frames_sent.entry(actor.to_string()).or_insert(0) += 1;
    }

    /// Record that a frame was received by an actor.
    pub fn record_frame_received(&mut self, actor: &str) {
        *self.frames_received.entry(actor.to_string()).or_insert(0) += 1;
    }

    /// Record a network event.
    pub fn record_network_event(&mut self, event: NetworkEvent) {
        self.network_events.push(event);
    }

    /// Get number of frames sent by an actor.
    pub fn frames_sent(&self, actor: &str) -> usize {
        self.frames_sent.get(actor).copied().unwrap_or(0)
    }

    /// Get number of frames received by an actor.
    pub fn frames_received(&self, actor: &str) -> usize {
        self.frames_received.get(actor).copied().unwrap_or(0)
    }

    /// Get all network events that occurred.
    pub fn network_events(&self) -> &[NetworkEvent] {
        &self.network_events
    }

    /// Check if all actors are in Authenticated state.
    pub fn all_authenticated(&self) -> bool {
        let clients_ok = self.clients.values().all(|c| c.state() == ConnectionState::Authenticated);
        let servers_ok = self.servers.values().all(|s| s.state() == ConnectionState::Authenticated);
        clients_ok && servers_ok
    }

    /// Check if all actors have matching session IDs.
    pub fn session_ids_match(&self) -> bool {
        let mut session_ids: Vec<Option<u64>> = Vec::new();

        for client in self.clients.values() {
            session_ids.push(client.session_id());
        }
        for server in self.servers.values() {
            session_ids.push(server.session_id());
        }

        // Filter out None values
        let ids: Vec<u64> = session_ids.into_iter().flatten().collect();

        // All non-None IDs should be the same
        if ids.is_empty() {
            return false;
        }

        let first = ids[0];
        ids.iter().all(|&id| id == first)
    }

    /// Get all actor names (clients and servers).
    pub fn actor_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        names.extend(self.clients.keys().cloned());
        names.extend(self.servers.keys().cloned());
        names
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
