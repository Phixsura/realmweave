//! High-level `Game` facade: a board graph + ruleset + state, with history,
//! undo, serialization, and deterministic replay.

use serde::{Deserialize, Serialize};

use crate::board::{BoardGraph, NodeId, Player};
use crate::rules::{self, RuleError, RuleSet};
use crate::state::{GameConfig, GameResult, GameState, Move};

/// Errors from [`Game`] operations.
#[derive(Debug, thiserror::Error)]
pub enum GameError {
    /// Record's board fingerprint doesn't match the regenerated board:
    /// the generator changed since the game was played.
    #[error("board {board} content changed since this record was made (generator drift)")]
    BoardDrift {
        /// The board id whose content no longer matches.
        board: String,
    },
    /// The ruleset rejected a move.
    #[error(transparent)]
    Rule(#[from] RuleError),
    /// Config and board graph disagree about the board id.
    #[error("board id mismatch: config wants {expected}, graph is {actual}")]
    BoardMismatch {
        /// Board id the config asked for.
        expected: String,
        /// Board id the graph actually has.
        actual: String,
    },
    /// `undo` with an empty history.
    #[error("nothing to undo")]
    NothingToUndo,
    /// Ruleset and board goal-structure don't match (e.g. trinity on a
    /// hex board, or an origin ruleset on a side-goal board).
    #[error("ruleset {ruleset} cannot play board {board}")]
    IncompatibleBoard {
        /// The requested ruleset id.
        ruleset: String,
        /// The offending board id.
        board: String,
    },
}

/// Serializable record from which a finished (or in-progress) game can be
/// reconstructed exactly. This is the persistence/replay format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    /// Match configuration (ruleset id, board id, options).
    pub config: GameConfig,
    /// Content fingerprint of the board this was played on (None on
    /// records predating the field). Checked on replay: a mismatch means
    /// the generator changed and the record is unsafe to trust.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_fingerprint: Option<u64>,
    /// Ordered move log.
    pub moves: Vec<Move>,
    /// Result, if the game ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
}

/// A live game: board + ruleset + state, with undo history.
pub struct Game {
    board: BoardGraph,
    ruleset: Box<dyn RuleSet>,
    config: GameConfig,
    state: GameState,
    /// Snapshots before each move, for undo (local/debug tooling).
    history: Vec<GameState>,
}

impl Game {
    /// Start a fresh game; origins are pre-placed and the ruleset's
    /// `setup` is applied.
    pub fn new(board: BoardGraph, config: GameConfig) -> Result<Self, GameError> {
        if board.definition().id != config.board_id {
            return Err(GameError::BoardMismatch {
                expected: config.board_id.clone(),
                actual: board.definition().id.clone(),
            });
        }
        let ruleset = rules::ruleset_by_id(&config.ruleset_id, config.pie_rule)?;
        // Ruleset/board compatibility, by board FAMILY (not just origin
        // presence): trinity and triforce boards both lack origins, but
        // their geometry functions index differently — trinity rules on a
        // merged-triangle board (or vice versa) compute garbage realm
        // partitions and bogus wins. A mismatch produces silent nonsense
        // games, so reject it here.
        let family = board.definition().family();
        let wanted = match config.ruleset_id.as_str() {
            rules::TRINITY_Y_V4 => crate::board::BoardFamily::SplitTriangles,
            rules::TRIFORCE_V5 => crate::board::BoardFamily::MergedTriangle,
            _ => crate::board::BoardFamily::StackedHex,
        };
        if family != wanted {
            return Err(GameError::IncompatibleBoard {
                ruleset: config.ruleset_id.clone(),
                board: board.definition().id.clone(),
            });
        }
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

    /// The board graph.
    pub fn board(&self) -> &BoardGraph {
        &self.board
    }

    /// The match configuration.
    pub fn config(&self) -> &GameConfig {
        &self.config
    }

    /// The active ruleset (capability queries and versioned id).
    pub fn rules(&self) -> &dyn RuleSet {
        self.ruleset.as_ref()
    }

    /// Current state snapshot.
    pub fn state(&self) -> &GameState {
        &self.state
    }

    /// Whose turn it is.
    pub fn to_move(&self) -> Player {
        self.state.to_move
    }

    /// Terminal result, if the game has ended.
    pub fn result(&self) -> Option<GameResult> {
        self.ruleset.evaluate(&self.board, &self.state)
    }

    /// All currently legal moves.
    pub fn legal_moves(&self) -> Vec<Move> {
        self.ruleset.legal_moves(&self.board, &self.state)
    }

    /// Check one move without applying it.
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

    /// `player`'s connected component containing `start` (UI helper).
    pub fn connected_component(&self, player: Player, start: NodeId) -> Vec<NodeId> {
        rules::connected_component(&self.board, &self.state, player, start)
    }

    /// All of `player`'s connected components (UI helper).
    pub fn player_components(&self, player: Player) -> Vec<Vec<NodeId>> {
        rules::player_components(&self.board, &self.state, player)
    }

    /// Whether `player` currently has a full-graph realm weave.
    pub fn has_realm_weave(&self, player: Player) -> bool {
        rules::has_realm_weave(&self.board, &self.state, player)
    }

    /// Serializable record: config + ordered move log (+ result).
    pub fn record(&self) -> GameRecord {
        GameRecord {
            config: self.config.clone(),
            board_fingerprint: Some(self.board.definition().fingerprint()),
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
            // No undo history on replays: keeping every intermediate
            // snapshot made replay O(n²) in memory for zero benefit
            // (bot workers and review caches replay constantly).
            let next = game.ruleset.apply_move(&game.board, &game.state, mv)?;
            game.state = next;
        }
        Ok(game)
    }

    /// Replay a full record with board-content verification: if the record
    /// carries a fingerprint and the provided board doesn't match, the
    /// replay is refused — a silent generator drift would otherwise
    /// reproduce a subtly different game.
    pub fn replay_record(board: BoardGraph, record: &GameRecord) -> Result<Self, GameError> {
        if let Some(fp) = record.board_fingerprint {
            if board.definition().fingerprint() != fp {
                return Err(GameError::BoardDrift {
                    board: board.definition().id.clone(),
                });
            }
        }
        Self::replay(board, record.config.clone(), &record.moves)
    }
}
