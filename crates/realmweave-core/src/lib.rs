//! Realmweave core engine: board graph, game state, moves, and rules.
//!
//! This crate has **zero dependency** on UI, networking, databases, or async
//! runtimes. It is the single authoritative implementation of the rules,
//! reused by the server, the client, simulations, and tooling.
//!
//! > The world is a graph; strategy emerges from how players weave and sever
//! > paths through it.

pub mod board;
pub mod boardgen;
pub mod bot;
pub mod game;
pub mod rules;
pub mod state;
pub mod validate;

pub use board::{
    BoardDefinition, BoardError, BoardGraph, Edge, EdgeKind, Node, NodeId, Origin, Player, Realm,
};
pub use game::{Game, GameError, GameRecord};
pub use rules::{
    RuleError, RuleSet, WeaveRules, ALL_RULESETS, SEVER_V1, THREE_REALMS_V1, TRINITY_Y_V4,
    WEAVE_LAYERS_V3, WEAVE_SEVER_V2,
};
pub use state::{GameConfig, GameResult, GameState, Move, TimeControl, WinReason};
pub use validate::{validate_board, ValidationError};
