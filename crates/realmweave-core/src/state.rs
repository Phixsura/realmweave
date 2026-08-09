//! Game state, moves, configuration, and results.
//!
//! State transitions are deterministic: identical config + move log always
//! reproduces the identical state. Snapshots are cheap `Clone`s.

use serde::{Deserialize, Serialize};

use crate::board::{NodeId, Player};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Move {
    /// Place a stone on an empty node.
    Place(NodeId),
    /// Permanently remove an edge from the board (weave-sever-v2; consumes
    /// a scissor). The u32 is the edge's index in `BoardDefinition.edges`.
    CutEdge(u32),
    /// Remove an enemy non-origin stone (sever ruleset; consumes a charge).
    Sever(NodeId),
    /// Decline to move (territory ruleset; two consecutive passes end the game).
    Pass,
    /// Pie rule: second player swaps sides instead of moving.
    Swap,
    Resign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WinReason {
    RealmWeave,
    /// Opponent's origins can never be connected again (weave-sever-v2).
    Strangle,
    /// Higher territory score after the game closed (territory ruleset).
    Territory,
    Resignation,
    Timeout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameResult {
    Win {
        player: Player,
        reason: WinReason,
    },
    /// Board full (or agreed) with no weave: drawn.
    Draw,
}

/// Data-driven chess-style clock configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeControl {
    pub base_ms: u64,
    pub increment_ms: u64,
}

impl TimeControl {
    pub const QUICK: TimeControl = TimeControl {
        base_ms: 12 * 60 * 1000,
        increment_ms: 5 * 1000,
    };
    pub const STANDARD: TimeControl = TimeControl {
        base_ms: 40 * 60 * 1000,
        increment_ms: 15 * 1000,
    };
    pub const GRAND: TimeControl = TimeControl {
        base_ms: 70 * 60 * 1000,
        increment_ms: 30 * 1000,
    };
}

/// Per-match configuration. The ruleset id is versioned so persisted games
/// always know which evaluator produced them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameConfig {
    pub ruleset_id: String,
    pub board_id: String,
    pub pie_rule: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_control: Option<TimeControl>,
}

impl GameConfig {
    pub fn new(board_id: impl Into<String>) -> Self {
        GameConfig {
            ruleset_id: crate::rules::THREE_REALMS_V1.to_string(),
            board_id: board_id.into(),
            pie_rule: false,
            time_control: None,
        }
    }

    pub fn with_pie_rule(mut self, pie: bool) -> Self {
        self.pie_rule = pie;
        self
    }

    pub fn with_ruleset(mut self, ruleset_id: impl Into<String>) -> Self {
        self.ruleset_id = ruleset_id.into();
        self
    }

    pub fn with_time_control(mut self, tc: TimeControl) -> Self {
        self.time_control = Some(tc);
        self
    }
}

/// Complete game state. Occupancy is indexed by dense `NodeId`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    pub board_id: String,
    pub occupancy: Vec<Option<Player>>,
    pub to_move: Player,
    /// Full ordered move history.
    pub move_log: Vec<Move>,
    /// Player who completed a provisional Realm Weave and is waiting for it
    /// to survive the opponent's response turn.
    pub pending_weave: Option<Player>,
    pub result: Option<GameResult>,
    /// Whether the pie-rule swap has been consumed (or forfeited).
    pub swap_used: bool,
    /// Number of moves played (including Swap).
    pub ply: u32,
    /// Remaining sever charges per player (sever ruleset; [Light, Dark]).
    #[serde(default)]
    pub sever_charges: [u8; 2],
    /// Consecutive passes (territory ruleset; 2 ends the game).
    #[serde(default)]
    pub consecutive_passes: u8,
    /// Hashes of all previous positions (supply ruleset positional superko).
    #[serde(default)]
    pub position_hashes: Vec<u64>,
    /// Stones captured BY each player ([by Light, by Dark]; supply ruleset).
    #[serde(default)]
    pub captures: [u32; 2],
    /// Edges removed from the board (weave-sever-v2), by edge index.
    #[serde(default)]
    pub cut_edges: Vec<u32>,
    /// Remaining scissors per player ([Light, Dark]; weave-sever-v2).
    #[serde(default)]
    pub scissors: [u8; 2],
}

impl GameState {
    pub fn new(board_id: impl Into<String>, node_count: usize) -> Self {
        GameState {
            board_id: board_id.into(),
            occupancy: vec![None; node_count],
            to_move: Player::Light,
            move_log: Vec::new(),
            pending_weave: None,
            result: None,
            swap_used: false,
            ply: 0,
            sever_charges: [0, 0],
            consecutive_passes: 0,
            position_hashes: Vec::new(),
            captures: [0, 0],
            cut_edges: Vec::new(),
            scissors: [0, 0],
        }
    }

    pub fn occupant(&self, node: NodeId) -> Option<Player> {
        self.occupancy[node as usize]
    }

    pub fn is_finished(&self) -> bool {
        self.result.is_some()
    }

    pub fn stones_of(&self, player: Player) -> Vec<NodeId> {
        self.occupancy
            .iter()
            .enumerate()
            .filter_map(|(i, occ)| (*occ == Some(player)).then_some(i as NodeId))
            .collect()
    }
}
