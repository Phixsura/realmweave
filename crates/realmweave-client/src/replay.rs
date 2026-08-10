//! Replay viewing: a loaded `GameRecord` plus a cursor. Pure logic, no Bevy.

use realmweave_core::{boardgen, BoardGraph, Game, GameRecord};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Annotation {
    pub ply: u32,
    pub player: String,
    pub text: String,
}

pub struct ReplayState {
    pub record: GameRecord,
    /// Number of moves currently applied (0..=record.moves.len()).
    pub cursor: usize,
    /// Optional per-move commentary (from `<record>.notes.json`).
    pub annotations: Vec<Annotation>,
    /// Auto-advance: seconds per move (demo mode). 0 = manual.
    pub auto_seconds: f32,
    /// Countdown to the next auto step.
    pub auto_timer: f32,
}

impl ReplayState {
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let record: GameRecord = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        // Validate the record replays cleanly before accepting it.
        let game = Self::build(&record, record.moves.len())?;
        if record.result.is_some() && game.result() != record.result {
            return Err("record result does not match replayed result".to_string());
        }
        // Sidecar annotations: `<path minus .json>.notes.json`.
        let notes_path = path
            .strip_suffix(".json")
            .map(|stem| format!("{stem}.notes.json"))
            .unwrap_or_else(|| format!("{path}.notes.json"));
        let annotations = std::fs::read_to_string(&notes_path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Ok(ReplayState {
            record,
            cursor: 0,
            annotations,
            auto_seconds: 0.0,
            auto_timer: 0.0,
        })
    }

    /// Commentary for the move that produced the current position.
    pub fn current_annotation(&self) -> Option<&Annotation> {
        if self.cursor == 0 {
            return None;
        }
        self.annotations
            .iter()
            .find(|a| a.ply == self.cursor as u32)
    }

    pub fn len(&self) -> usize {
        self.record.moves.len()
    }

    /// Board for the record's board id: every generated family regenerates
    /// deterministically (hex + trinity); unknown ids fall back to a local
    /// file in `boards/` (hand-made boards).
    fn board(record: &GameRecord) -> Result<BoardGraph, String> {
        let id = &record.config.board_id;
        if let Some(def) = boardgen::resolve(id) {
            return BoardGraph::new(def).map_err(|e| e.to_string());
        }
        let path = format!("boards/{id}.json");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("unknown board {id} (also tried {path}: {e})"))?;
        let def = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        BoardGraph::new(def).map_err(|e| e.to_string())
    }

    fn build(record: &GameRecord, upto: usize) -> Result<Game, String> {
        let board = Self::board(record)?;
        Game::replay(board, record.config.clone(), &record.moves[..upto]).map_err(|e| e.to_string())
    }

    /// Game state at the current cursor.
    pub fn game_at_cursor(&self) -> Result<Game, String> {
        Self::build(&self.record, self.cursor)
    }

    pub fn seek(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.len());
    }
}
