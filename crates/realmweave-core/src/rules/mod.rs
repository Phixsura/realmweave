//! Rule sets. One ruleset per file; this module owns the trait, the error
//! type, versioned ids, and the registry. Shared graph predicates live in
//! `shared.rs`.
//!
//! Design discipline (docs/design-trinity-y-v4.md): a ruleset earns its
//! place with a small number of orthogonal rules — depth must emerge, not
//! be patched in. Dead experiments are deleted, not flag-gated; git
//! history is the museum.

mod classic;
mod shared;
mod trinity_y;
mod weave_sever;

pub use classic::WeaveRules;
pub use shared::*;
pub use trinity_y::TrinityY;
pub use weave_sever::WeaveSeverV2;

use crate::board::{BoardGraph, NodeId, Player};
use crate::state::{GameResult, GameState, Move};

/// Classic weave race (docs/rules.md).
pub const THREE_REALMS_V1: &str = "three-realms-v1";
/// Classic + stone-removal charges.
pub const SEVER_V1: &str = "three-realms-sever-v1";
/// Scissors edge-cutting + strangle (docs/design-weave-sever-v2.md).
pub const WEAVE_SEVER_V2: &str = "weave-sever-v2";
/// Petrifying layer scoring (docs/design-weave-layers-v3.md).
pub const WEAVE_LAYERS_V3: &str = "weave-layers-v3";
/// Flagship: Y goal + liberties (docs/design-trinity-y-v4.md).
pub const TRINITY_Y_V4: &str = "trinity-y-v4";

/// Sever charges per player in the sever variant.
pub const SEVER_CHARGES: u8 = 3;
/// Scissors per player in weave-sever-v2. Origin-adjacent edges are
/// uncuttable, making the min origin-pair edge cut 8 (measured on all
/// standard boards) — pure-scissor strangling is impossible at K=3;
/// strangles require stone walls plus surgical cuts.
pub const SCISSORS: u8 = 3;
/// Scissors granted to BOTH players when a layer petrifies (v3).
pub const LAYER_SCISSORS: u8 = 2;
/// Scissors cap (v3).
pub const SCISSORS_CAP: u8 = 4;
/// Layers needed to win weave-layers-v3.
pub const LAYERS_TO_WIN: u8 = 3;
/// Hard ply cap for weave-layers-v3: at this many moves the game closes
/// and fallback scoring (layers first) decides. Keeps marathons bounded.
pub const V3_PLY_CAP: u32 = 500;

/// A move rejected by a ruleset. Variant messages are self-describing.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[allow(missing_docs)] // each variant's #[error] text IS its documentation
pub enum RuleError {
    #[error("game is already finished")]
    GameFinished,
    #[error("node {0} is occupied")]
    Occupied(NodeId),
    #[error("node {0} does not exist")]
    NoSuchNode(NodeId),
    #[error("swap is not available")]
    SwapUnavailable,
    #[error("sever is not available")]
    SeverUnavailable,
    #[error("node {0} cannot be severed")]
    CannotSever(NodeId),
    #[error("pass is not allowed in this ruleset")]
    PassUnavailable,
    #[error("move at {0} would leave your own group without supply")]
    SuicideMove(NodeId),
    #[error("move at {0} repeats a previous position (ko)")]
    KoViolation(NodeId),
    #[error("edge {0} cannot be cut")]
    CannotCut(u32),
    #[error("node {0} is adjacent to an enemy origin (sanctum)")]
    OriginSanctum(NodeId),
    #[error("no scissors remaining")]
    NoScissors,
    #[error("this cut would strangle your own origins")]
    SelfStrangle,
    #[error("unknown ruleset id {0}")]
    UnknownRuleset(String),
}

/// A versioned rule implementation. The engine, server, client, and sim
/// all consume rules exclusively through this trait.
pub trait RuleSet: Send + Sync {
    /// Stable versioned identifier persisted with every game.
    fn id(&self) -> &str;
    /// One-time state initialization (e.g. sever charges).
    fn setup(&self, _state: &mut GameState) {}
    /// Every legal move in the current position.
    fn legal_moves(&self, board: &BoardGraph, state: &GameState) -> Vec<Move>;
    /// Check a single move without applying it.
    fn validate_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<(), RuleError>;
    /// Apply a validated move, producing the next state.
    fn apply_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<GameState, RuleError>;
    /// Terminal result, if the game has ended.
    fn evaluate(&self, board: &BoardGraph, state: &GameState) -> Option<GameResult>;
}

/// Look up a ruleset implementation by its persisted id.
pub fn ruleset_by_id(id: &str, pie_rule: bool) -> Result<Box<dyn RuleSet>, RuleError> {
    match id {
        THREE_REALMS_V1 => Ok(Box::new(WeaveRules::classic(pie_rule))),
        SEVER_V1 => Ok(Box::new(WeaveRules::sever(pie_rule))),
        WEAVE_SEVER_V2 => Ok(Box::new(WeaveSeverV2 {
            pie_rule,
            layers_to_win: 1,
        })),
        WEAVE_LAYERS_V3 => Ok(Box::new(WeaveSeverV2 {
            pie_rule,
            layers_to_win: LAYERS_TO_WIN,
        })),
        TRINITY_Y_V4 => Ok(Box::new(TrinityY { pie_rule })),
        other => Err(RuleError::UnknownRuleset(other.to_string())),
    }
}

/// All known ruleset ids (for tooling/UI).
pub const ALL_RULESETS: [&str; 5] = [
    TRINITY_Y_V4,
    WEAVE_LAYERS_V3,
    WEAVE_SEVER_V2,
    THREE_REALMS_V1,
    SEVER_V1,
];

pub(crate) fn player_index(player: Player) -> usize {
    match player {
        Player::Light => 0,
        Player::Dark => 1,
    }
}
