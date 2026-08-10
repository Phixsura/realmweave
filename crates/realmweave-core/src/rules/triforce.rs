//! Triforce v5 — the flagship distilled to its purest form.
//!
//! ONE connected battlefield (the merged triangle: three corner realms +
//! the central weave-heart), TWO rules:
//!
//!   1. Place a stone on any empty node (pie swap available to Dark).
//!      DEATH: a group with no liberties is captured; suicide illegal;
//!      recreating a previous position illegal (positional superko).
//!   2. WEAVE: connect all three sides of the great triangle with one
//!      group. First weave wins. The Y theorem guarantees exactly one
//!      player can ever do this on a full board — no draws, and blocking
//!      IS building.
//!
//! The realms are geography, not sub-games: every winning path crosses
//! realms and contests the heart (measured 100%/100% on random fills —
//! docs/research-triforce.md). Docs: design-triforce-v5.md.

use std::collections::VecDeque;

use crate::board::{BoardGraph, NodeId, Player};
use crate::state::{GameResult, GameState, Move, WinReason};

use super::*;

/// Triforce ruleset (see module docs).
pub struct Triforce {
    /// Whether Dark may swap as its first response.
    pub pie_rule: bool,
}

impl Triforce {
    /// Big-triangle side length (deepest axial row + 1; pierced-safe).
    fn side_of(board: &BoardGraph) -> usize {
        crate::boardgen::tf_side_len(board.definition())
    }

    /// Who (if anyone) has woven the great triangle: one group touching
    /// all three of the big sides.
    pub fn weaver(board: &BoardGraph, state: &GameState) -> Option<Player> {
        let side = Self::side_of(board);
        let n = board.node_count();
        for player in [Player::Light, Player::Dark] {
            let mut visited = vec![false; n];
            for start in 0..n as NodeId {
                if state.occupant(start) != Some(player) || visited[start as usize] {
                    continue;
                }
                let mut queue = VecDeque::new();
                visited[start as usize] = true;
                queue.push_back(start);
                let mut touch = crate::boardgen::triforce_sides(board.definition(), side, start);
                while let Some(cur) = queue.pop_front() {
                    for &nb in board.neighbors(cur) {
                        if !visited[nb as usize] && state.occupant(nb) == Some(player) {
                            visited[nb as usize] = true;
                            queue.push_back(nb);
                            touch |= crate::boardgen::triforce_sides(board.definition(), side, nb);
                        }
                    }
                }
                if touch == 7 {
                    return Some(player);
                }
            }
        }
        None
    }

    /// Best sides-touched count for `player` (0..=3) — the HUD's weave
    /// progress meter. 3 = woven. Returns (sides, group_len) for the best
    /// group so callers can distinguish a lone corner stone (which touches
    /// two sides by geometry) from a genuine two-sided weave.
    pub fn weave_progress(board: &BoardGraph, state: &GameState, player: Player) -> (u32, usize) {
        let side = Self::side_of(board);
        let n = board.node_count();
        let mut visited = vec![false; n];
        let mut best = (0u32, 0usize);
        for start in 0..n as NodeId {
            if state.occupant(start) != Some(player) || visited[start as usize] {
                continue;
            }
            let mut queue = VecDeque::from([start]);
            visited[start as usize] = true;
            let mut len = 1usize;
            let mut touch = crate::boardgen::triforce_sides(board.definition(), side, start);
            while let Some(cur) = queue.pop_front() {
                for &nb in board.neighbors(cur) {
                    if !visited[nb as usize] && state.occupant(nb) == Some(player) {
                        visited[nb as usize] = true;
                        queue.push_back(nb);
                        len += 1;
                        touch |= crate::boardgen::triforce_sides(board.definition(), side, nb);
                    }
                }
            }
            let cand = (touch.count_ones(), len);
            if cand > best {
                best = cand;
            }
        }
        best
    }

    fn swap_available(&self, state: &GameState) -> bool {
        self.pie_rule && !state.swap_used && state.ply == 1 && state.to_move == Player::Dark
    }

    /// The group containing `start` and whether it has any liberty.
    fn group_liberties(
        board: &BoardGraph,
        occ: &[Option<Player>],
        start: NodeId,
    ) -> (Vec<NodeId>, bool) {
        let Some(player) = occ[start as usize] else {
            return (Vec::new(), true);
        };
        let mut members = vec![start];
        let mut visited = vec![false; board.node_count()];
        visited[start as usize] = true;
        let mut queue = VecDeque::from([start]);
        let mut has_liberty = false;
        while let Some(cur) = queue.pop_front() {
            for &nb in board.neighbors(cur) {
                match occ[nb as usize] {
                    None => has_liberty = true,
                    Some(p) if p == player && !visited[nb as usize] => {
                        visited[nb as usize] = true;
                        queue.push_back(nb);
                        members.push(nb);
                    }
                    _ => {}
                }
            }
        }
        (members, has_liberty)
    }

    /// Apply a placement with Go dynamics. Returns captured count, or None
    /// if the move is suicide.
    ///
    /// CONTRACT: on suicide (None) `occ` is left with the placed stone
    /// removed, but captures — which cannot have occurred on a suicide
    /// (capturing frees a liberty) — are asserted, not rolled back. Callers
    /// today pass throwaway clones anyway; the assert keeps a future
    /// in-place caller honest.
    fn place_with_capture(
        board: &BoardGraph,
        occ: &mut [Option<Player>],
        node: NodeId,
        player: Player,
    ) -> Option<u32> {
        occ[node as usize] = Some(player);
        let mut captured = 0u32;
        let mut checked = vec![false; board.node_count()];
        for k in 0..board.neighbors(node).len() {
            let nb = board.neighbors(node)[k];
            if occ[nb as usize] == Some(player.opponent()) && !checked[nb as usize] {
                let (members, alive) = Self::group_liberties(board, occ, nb);
                for &m in &members {
                    checked[m as usize] = true;
                }
                if !alive {
                    captured += members.len() as u32;
                    for m in members {
                        occ[m as usize] = None;
                    }
                }
            }
        }
        let (_, alive) = Self::group_liberties(board, occ, node);
        if !alive {
            debug_assert_eq!(captured, 0, "a capturing move always has a liberty");
            occ[node as usize] = None;
            return None;
        }
        Some(captured)
    }
}

impl RuleSet for Triforce {
    fn id(&self) -> &str {
        TRIFORCE_V5
    }

    fn setup(&self, _state: &mut GameState) {}

    fn legal_moves(&self, board: &BoardGraph, state: &GameState) -> Vec<Move> {
        if state.is_finished() {
            return Vec::new();
        }
        let mut moves: Vec<Move> = (0..board.node_count() as NodeId)
            .filter(|&n| {
                state.occupant(n).is_none()
                    && self.validate_move(board, state, &Move::Place(n)).is_ok()
            })
            .map(Move::Place)
            .collect();
        moves.push(Move::Pass);
        if self.swap_available(state) {
            moves.push(Move::Swap);
        }
        moves.push(Move::Resign);
        moves
    }

    fn validate_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<(), RuleError> {
        if state.is_finished() {
            return Err(RuleError::GameFinished);
        }
        match mv {
            Move::Place(node) => {
                if *node as usize >= board.node_count() {
                    return Err(RuleError::NoSuchNode(*node));
                }
                if state.occupant(*node).is_some() {
                    return Err(RuleError::Occupied(*node));
                }
                let mut occ = state.occupancy.clone();
                if Self::place_with_capture(board, &mut occ, *node, state.to_move).is_none() {
                    return Err(RuleError::SuicideMove(*node));
                }
                let mut sim = state.clone();
                sim.occupancy = occ;
                let hash = position_hash(&sim);
                if state.position_hashes.contains(&hash) {
                    return Err(RuleError::KoViolation(*node));
                }
                Ok(())
            }
            Move::Swap => {
                if self.swap_available(state) {
                    Ok(())
                } else {
                    Err(RuleError::SwapUnavailable)
                }
            }
            Move::Resign => Ok(()),
            Move::Pass => Ok(()),
            Move::CutEdge(e) => Err(RuleError::CannotCut(*e)),
            Move::Sever(n) => Err(RuleError::CannotSever(*n)),
        }
    }

    fn apply_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<GameState, RuleError> {
        self.validate_move(board, state, mv)?;
        let mover = state.to_move;
        let mut next = state.clone();
        next.move_log.push(*mv);
        next.ply += 1;
        if next.ply >= 2 {
            next.swap_used = true;
        }
        match mv {
            Move::Resign => {
                next.result = Some(GameResult::Win {
                    player: mover.opponent(),
                    reason: WinReason::Resignation,
                });
                return Ok(next);
            }
            Move::Swap => return Ok(next),
            Move::Pass => {
                next.consecutive_passes += 1;
                if next.consecutive_passes >= 2 {
                    // Y theorem: an unfinished board means nobody has woven;
                    // mutual passing concedes nothing decidable. Draw.
                    next.result = Some(GameResult::Draw);
                    return Ok(next);
                }
                next.to_move = mover.opponent();
                return Ok(next);
            }
            Move::Place(node) => {
                let captured =
                    Self::place_with_capture(board, &mut next.occupancy, *node, mover).unwrap_or(0); // suicide rejected by validate above
                next.captures[player_index(mover)] += captured;
                next.consecutive_passes = 0;
                next.position_hashes.push(position_hash(&next));
            }
            _ => unreachable!("validated"),
        }
        // Only the mover can newly weave: captures remove opponent stones
        // (which cannot connect the opponent's groups), and a pre-existing
        // opponent Y would have ended the game on their turn.
        if Self::weave_progress(board, &next, mover).0 == 3 {
            next.result = Some(GameResult::Win {
                player: mover,
                reason: WinReason::RealmWeave,
            });
        }
        next.to_move = mover.opponent();
        Ok(next)
    }

    fn evaluate(&self, _board: &BoardGraph, state: &GameState) -> Option<GameResult> {
        state.result
    }

    fn allows_pass(&self) -> bool {
        true
    }
}
