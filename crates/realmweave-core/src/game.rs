//! High-level `Game` facade: a board graph + ruleset + state, with history,
//! undo, serialization, and deterministic replay.

use serde::{Deserialize, Serialize};

use crate::board::{BoardGraph, NodeId, Player};
use crate::rules::{self, RuleError, RuleSet};
use crate::state::{GameConfig, GameResult, GameState, Move};

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error(transparent)]
    Rule(#[from] RuleError),
    #[error("board id mismatch: config wants {expected}, graph is {actual}")]
    BoardMismatch { expected: String, actual: String },
    #[error("nothing to undo")]
    NothingToUndo,
}

/// Serializable record from which a finished (or in-progress) game can be
/// reconstructed exactly. This is the persistence/replay format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    pub config: GameConfig,
    pub moves: Vec<Move>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
}

pub struct Game {
    board: BoardGraph,
    ruleset: Box<dyn RuleSet>,
    config: GameConfig,
    state: GameState,
    /// Snapshots before each move, for undo (local/debug tooling).
    history: Vec<GameState>,
}

impl Game {
    pub fn new(board: BoardGraph, config: GameConfig) -> Result<Self, GameError> {
        if board.definition().id != config.board_id {
            return Err(GameError::BoardMismatch {
                expected: config.board_id.clone(),
                actual: board.definition().id.clone(),
            });
        }
        let ruleset = rules::ruleset_by_id(&config.ruleset_id, config.pie_rule)?;
        let mut state = GameState::new(config.board_id.clone(), board.node_count());
        // Origins are pre-occupied by their owners and are part of the
        // player's network from the start.
        for origin in &board.definition().origins {
            state.occupancy[origin.node as usize] = Some(origin.player);
        }
        ruleset.setup(&mut state);
        Ok(Game {
            board,
            ruleset,
            config,
            state,
            history: Vec::new(),
        })
    }

    pub fn board(&self) -> &BoardGraph {
        &self.board
    }

    pub fn config(&self) -> &GameConfig {
        &self.config
    }

    pub fn state(&self) -> &GameState {
        &self.state
    }

    pub fn to_move(&self) -> Player {
        self.state.to_move
    }

    pub fn result(&self) -> Option<GameResult> {
        self.ruleset.evaluate(&self.board, &self.state)
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        self.ruleset.legal_moves(&self.board, &self.state)
    }

    pub fn validate(&self, mv: &Move) -> Result<(), RuleError> {
        self.ruleset.validate_move(&self.board, &self.state, mv)
    }

    /// Apply a move, recording history for undo.
    pub fn play(&mut self, mv: Move) -> Result<&GameState, GameError> {
        let next = self.ruleset.apply_move(&self.board, &self.state, &mv)?;
        self.history.push(std::mem::replace(&mut self.state, next));
        Ok(&self.state)
    }

    /// Pie-rule swap convenience wrapper.
    pub fn swap_sides(&mut self) -> Result<&GameState, GameError> {
        self.play(Move::Swap)
    }

    /// Undo the last move (local/debug tooling only).
    pub fn undo(&mut self) -> Result<&GameState, GameError> {
        let prev = self.history.pop().ok_or(GameError::NothingToUndo)?;
        self.state = prev;
        Ok(&self.state)
    }

    pub fn connected_component(&self, player: Player, start: NodeId) -> Vec<NodeId> {
        rules::connected_component(&self.board, &self.state, player, start)
    }

    pub fn player_components(&self, player: Player) -> Vec<Vec<NodeId>> {
        rules::player_components(&self.board, &self.state, player)
    }

    pub fn has_realm_weave(&self, player: Player) -> bool {
        rules::has_realm_weave(&self.board, &self.state, player)
    }

    /// Serializable record: config + ordered move log (+ result).
    pub fn record(&self) -> GameRecord {
        GameRecord {
            config: self.config.clone(),
            moves: self.state.move_log.clone(),
            result: self.state.result,
        }
    }

    /// Deterministically rebuild a game from config + move log.
    pub fn replay(
        board: BoardGraph,
        config: GameConfig,
        moves: &[Move],
    ) -> Result<Self, GameError> {
        let mut game = Game::new(board, config)?;
        for mv in moves {
            game.play(*mv)?;
        }
        Ok(game)
    }
}
