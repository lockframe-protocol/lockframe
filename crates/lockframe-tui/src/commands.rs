//! Command parsing for TUI and other text-based interfaces.
//!
//! This module parses command strings into structured [`Command`] values.

use lockframe_core::mls::RoomId;

/// Parsed command from user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Connect to the server.
    Connect,

    /// Create a new room.
    CreateRoom {
        /// 128-bit room UUID.
        room_id: RoomId,
    },

    /// Join an existing room via external commit.
    JoinRoom {
        /// 128-bit room UUID.
        room_id: RoomId,
    },

    /// Leave the active room.
    LeaveActiveRoom,

    /// Publish a key package to the server.
    PublishKeyPackage,

    /// Add a member to the active room.
    AddMember {
        /// User ID to add.
        user_id: u64,
    },

    /// Quit the application.
    Quit,

    /// Send a message to the active room.
    Message {
        /// Message content.
        content: String,
    },

    /// Unknown or invalid command.
    Unknown {
        /// The original input.
        input: String,
    },

    /// Command with missing or invalid arguments.
    InvalidArgs {
        /// Command name.
        command: String,
        /// Error message.
        error: String,
    },
}

/// Parse a user input string into a command.
///
/// Commands start with `/`. Anything else is treated as a message.
pub fn parse(input: &str) -> Command {
    let input = input.trim();

    if input.is_empty() {
        return Command::Message { content: String::new() };
    }

    let Some(cmd_str) = input.strip_prefix('/') else {
        return Command::Message { content: input.to_string() };
    };

    let parts: Vec<&str> = cmd_str.split_whitespace().collect();
    let command = parts.first().copied().unwrap_or("");

    match command {
        "connect" => Command::Connect,

        "create" => match parts.get(1) {
            Some(id_str) => match id_str.parse::<u128>() {
                Ok(room_id) => Command::CreateRoom { room_id },
                Err(_) => Command::InvalidArgs {
                    command: "create".into(),
                    error: "Invalid room ID".into(),
                },
            },
            None => Command::InvalidArgs {
                command: "create".into(),
                error: "Usage: /create <room_id>".into(),
            },
        },

        "join" => match parts.get(1) {
            Some(id_str) => match id_str.parse::<u128>() {
                Ok(room_id) => Command::JoinRoom { room_id },
                Err(_) => {
                    Command::InvalidArgs { command: "join".into(), error: "Invalid room ID".into() }
                },
            },
            None => Command::InvalidArgs {
                command: "join".into(),
                error: "Usage: /join <room_id>".into(),
            },
        },

        "leave" => Command::LeaveActiveRoom,

        "publish" => Command::PublishKeyPackage,

        "add" => match parts.get(1) {
            Some(id_str) => match id_str.parse::<u64>() {
                Ok(user_id) => Command::AddMember { user_id },
                Err(_) => {
                    Command::InvalidArgs { command: "add".into(), error: "Invalid user ID".into() }
                },
            },
            None => Command::InvalidArgs {
                command: "add".into(),
                error: "Usage: /add <user_id>".into(),
            },
        },

        "quit" | "q" => Command::Quit,

        _ => Command::Unknown { input: input.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_message() {
        assert_eq!(parse("hello world"), Command::Message { content: "hello world".into() });
    }

    #[test]
    fn parse_connect() {
        assert_eq!(parse("/connect"), Command::Connect);
    }

    #[test]
    fn parse_create_room() {
        assert_eq!(parse("/create 100"), Command::CreateRoom { room_id: 100 });
    }

    #[test]
    fn parse_create_room_missing_id() {
        assert!(
            matches!(parse("/create"), Command::InvalidArgs { command, .. } if command == "create")
        );
    }

    #[test]
    fn parse_join_room() {
        assert_eq!(parse("/join 200"), Command::JoinRoom { room_id: 200 });
    }

    #[test]
    fn parse_leave() {
        assert_eq!(parse("/leave"), Command::LeaveActiveRoom);
    }

    #[test]
    fn parse_add_member() {
        assert_eq!(parse("/add 42"), Command::AddMember { user_id: 42 });
    }

    #[test]
    fn parse_quit() {
        assert_eq!(parse("/quit"), Command::Quit);
        assert_eq!(parse("/q"), Command::Quit);
    }

    #[test]
    fn parse_unknown_command() {
        assert!(matches!(parse("/unknown"), Command::Unknown { .. }));
    }

    #[test]
    fn parse_empty() {
        assert_eq!(parse(""), Command::Message { content: String::new() });
    }
}
