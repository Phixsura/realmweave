//! Realmweave core engine: board graph, game state, moves, and rules.
//!
//! This crate has **zero dependency** on UI, networking, databases, or async
//! runtimes. It is the single authoritative implementation of the rules,
//! reused by the server, the client, simulations, and tooling.
//!
//! > The world is a graph; strategy emerges from how players weave and sever
//! > paths through it.
//!
//! # Quick start
//!
//! ```
//! use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, TRINITY_Y_V4};
//!
//! // A trinity board: three triangular realms, side length 7.
//! let def = boardgen::generate_trinity(7).expect("supported size");
//! let board = BoardGraph::new(def).expect("valid board");
//! let config = GameConfig::new(board.definition().id.clone()).with_ruleset(TRINITY_Y_V4);
//! let mut game = Game::new(board, config).expect("game starts");
//!
//! // Play the first legal placement.
//! let mv = game
//!     .legal_moves()
//!     .into_iter()
//!     .find(|m| matches!(m, Move::Place(_)))
//!     .expect("placements available");
//! game.play(mv).expect("legal moves apply");
//!
//! // Determinism: config + move log reproduces the exact state.
//! let def = boardgen::generate_trinity(7).expect("supported size");
//! let board = realmweave_core::BoardGraph::new(def).expect("valid board");
//! let replay = Game::replay(board, game.config().clone(), &game.state().move_log)
//!     .expect("records replay");
//! assert_eq!(replay.state(), game.state());
//! ```

pub mod board;
pub mod boardgen;
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
