//! Versioned, typed client/server messages for Realmweave online play.
//!
//! The protocol is event-oriented: the client sends *intent*, never
//! authoritative state; the server broadcasts canonical events carrying
//! sequence numbers. Every envelope is versioned from the start.

#![allow(missing_docs)] // wire types: field names + serde tags are the contract

use serde::{Deserialize, Serialize};

use realmweave_core::{GameConfig, GameResult, Move, NodeId, Player, TimeControl};

pub const PROTOCOL_VERSION: u32 = 2;

/// Wire envelope. `seq` is a per-connection client command counter (client →
/// server) or the canonical room event number (server → client); the server
/// rejects duplicate or stale client sequence numbers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub v: u32,
    pub seq: u64,
    pub msg: T,
}

impl<T> Envelope<T> {
    pub fn new(seq: u64, msg: T) -> Self {
        Envelope {
            v: PROTOCOL_VERSION,
            seq,
            msg,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    CreateRoom {
        config: GameConfig,
    },
    JoinRoom {
        room_id: String,
    },
    /// Resume a seat after a disconnect.
    Reconnect {
        room_id: String,
        token: String,
    },
    PlayMove {
        node: NodeId,
    },
    SeverStone {
        node: NodeId,
    },
    CutEdge {
        edge: u32,
    },
    Pass,
    SwapSides,
    Resign,
    Ping,
}

/// Remaining time per player, authoritative on the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockState {
    pub light_ms: u64,
    pub dark_ms: u64,
    /// Whose clock is running, if the game is live.
    pub running: Option<Player>,
}

impl ClockState {
    pub fn new(tc: TimeControl) -> Self {
        ClockState {
            light_ms: tc.base_ms,
            dark_ms: tc.base_ms,
            running: None,
        }
    }

    pub fn remaining(&self, player: Player) -> u64 {
        match player {
            Player::Light => self.light_ms,
            Player::Dark => self.dark_ms,
        }
    }
}

/// A committed move, broadcast to both seats.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoveEvent {
    /// Canonical room event number.
    pub seq: u64,
    /// 1-based ply of the move within the game.
    pub ply: u32,
    pub player: Player,
    pub mv: Move,
    pub clock: ClockState,
}

/// Everything a client needs to reconstruct the current game locally via
/// `realmweave_core::Game::replay`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub config: GameConfig,
    pub moves: Vec<Move>,
    pub clock: ClockState,
    /// Which color this recipient plays.
    pub seat: Player,
    pub opponent_connected: bool,
    pub started: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    RoomCreated {
        room_id: String,
        token: String,
        seat: Player,
    },
    Joined {
        room_id: String,
        token: String,
        seat: Player,
    },
    /// Full state after join/reconnect/start.
    Snapshot(GameSnapshot),
    MoveAccepted(MoveEvent),
    MoveRejected {
        reason: String,
    },
    ClockUpdate(ClockState),
    GameEnded {
        result: GameResult,
        clock: ClockState,
    },
    OpponentConnection {
        connected: bool,
    },
    Error {
        reason: String,
    },
    Pong,
}

/// Serialize a message into a JSON text frame.
pub fn encode<T: Serialize>(envelope: &Envelope<T>) -> String {
    serde_json::to_string(envelope).unwrap_or_default() // our types always serialize
}

/// Decode a JSON text frame, enforcing the protocol version.
pub fn decode<T: for<'de> Deserialize<'de>>(text: &str) -> Result<Envelope<T>, DecodeError> {
    let envelope: Envelope<T> = serde_json::from_str(text)?;
    if envelope.v != PROTOCOL_VERSION {
        return Err(DecodeError::VersionMismatch {
            expected: PROTOCOL_VERSION,
            actual: envelope.v,
        });
    }
    Ok(envelope)
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
    #[error("protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u32, actual: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_client_message() {
        let env = Envelope::new(7, ClientMessage::PlayMove { node: 42 });
        let text = encode(&env);
        let back: Envelope<ClientMessage> = decode(&text).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn round_trip_server_message() {
        let env = Envelope::new(
            3,
            ServerMessage::MoveAccepted(MoveEvent {
                seq: 3,
                ply: 1,
                player: Player::Light,
                mv: Move::Place(10),
                clock: ClockState {
                    light_ms: 1000,
                    dark_ms: 2000,
                    running: Some(Player::Dark),
                },
            }),
        );
        let text = encode(&env);
        let back: Envelope<ServerMessage> = decode(&text).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn rejects_wrong_version() {
        let mut env = Envelope::new(1, ClientMessage::Ping);
        env.v = 999;
        let text = serde_json::to_string(&env).unwrap();
        assert!(matches!(
            decode::<ClientMessage>(&text),
            Err(DecodeError::VersionMismatch { .. })
        ));
    }
}
