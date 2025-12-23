//! MLS operation payload types.
//!
//! These types wrap raw MLS protocol data. The actual MLS cryptographic
//! operations are handled by the `openmls` library in higher layers.

use serde::{Deserialize, Serialize};

/// Key package upload
///
/// Contains a serialized MLS KeyPackage for joining groups.
///
/// # Protocol Flow
///
/// Sent by a client who wants to join a room. The server stores this KeyPackage
/// and later includes it in a Welcome message when the client is added to the
/// group by another member's Commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyPackageData {
    /// Serialized MLS KeyPackage (from openmls)
    pub key_package_bytes: Vec<u8>,
}

/// MLS proposal
///
/// Proposals are staged changes to the group (add member, remove member, etc.)
///
/// # Protocol Flow
///
/// Proposals are sent to suggest changes but don't take effect immediately.
/// They must be "committed" by a Commit message. Flow:
/// 1. Member sends Proposal (e.g., Add, Remove, Update)
/// 2. Server validates and broadcasts to group
/// 3. Any member can send Commit referencing pending Proposals
/// 4. Commit advances the group epoch and applies all pending Proposals
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalData {
    /// Serialized MLS Proposal
    pub proposal_bytes: Vec<u8>,

    /// Proposal type hint (for routing/logging)
    pub proposal_type: ProposalType,
}

/// Type of MLS proposal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub enum ProposalType {
    /// Add a new member
    Add,
    /// Remove an existing member
    Remove,
    /// Update own key material
    Update,
    /// Pre-shared key
    PSK,
    /// Reinitialize the group with different parameters
    ReInit,
    /// External initialization proposal
    ExternalInit,
    /// Modify group context extensions
    GroupContextExtensions,
}

/// MLS commit
///
/// Commits apply one or more proposals and advance the epoch.
///
/// # Protocol Flow
///
/// Sent by a group member to finalize pending Proposals and advance the epoch:
/// 1. Member creates Commit referencing pending Proposals
/// 2. Server validates Commit (signatures, epoch match)
/// 3. Server sequences Commit with monotonic log_index
/// 4. Server broadcasts to all group members
/// 5. All members apply Commit and advance to new epoch
/// 6. Old epoch keys are deleted (forward secrecy)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitData {
    /// Serialized MLS Commit
    pub commit_bytes: Vec<u8>,

    /// New epoch number
    pub new_epoch: u64,

    /// Tree hash after commit
    pub tree_hash: [u8; 32],

    /// True if this is an external commit (from server or new joiner)
    pub is_external: bool,
}

/// MLS welcome message
///
/// Sent to new members joining the group.
///
/// # Protocol Flow
///
/// Sent to a newly added member after a Commit that included an Add proposal:
/// 1. Member A sends Add proposal (references member B's KeyPackage)
/// 2. Member A or another member sends Commit including the Add
/// 3. Server generates Welcome message encrypted to B's KeyPackage
/// 4. Server sends Welcome directly to B (not broadcast to group)
/// 5. B decrypts Welcome and joins the group at current epoch
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WelcomeData {
    /// Serialized MLS Welcome
    pub welcome_bytes: Vec<u8>,

    /// Epoch the new member will join at
    pub epoch: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_data_serde() {
        let commit = CommitData {
            commit_bytes: vec![1, 2, 3],
            new_epoch: 42,
            tree_hash: [0; 32],
            is_external: false,
        };

        let cbor = ciborium::ser::into_writer(&commit, Vec::new());
        assert!(cbor.is_ok());
    }
}
