//! Trinity Y v4.1 — one goal rule (Y theorem: attack IS defense) + one
//! dynamics rule (liberties: stones die). Docs: design-trinity-y-v4.md.

use std::collections::VecDeque;

use crate::board::{BoardGraph, NodeId, Player};
use crate::state::{GameResult, GameState, Move, WinReason};

use super::*;

// ------------------------------------------------------------- trinity-y ---

/// Trinity Y (v4.1) — the whole rulebook:
///
///   1. On your turn, place a stone on any empty node (or use the pie swap).
///   2. DEATH: a group with no empty adjacent node (no liberties) is
///      captured and removed. Your placement captures the enemy first;
///      a move that leaves your own group with no liberties is illegal
///      (suicide), and recreating a previous whole-board position is
///      illegal (positional superko) — the ko rule, exactly as in Go.
///   3. A realm is WON by the player whose single group connects all three
///      of that realm's sides (Y). Won realms are sealed: their stones are
///      immortal and the realm is closed to further play.
///   4. Win the match by winning TWO of the three realms.
///
/// One goal rule (Y — attack IS defense, no dead positions by theorem),
/// one dynamics rule (liberties — stones can die, walls need eyes, whole
/// groups can be hunted). Everything else — eyes, ladders, ko fights,
/// sacrifices, invasions of "finished" territory — must emerge.
pub struct TrinityY {
    /// Whether Dark may swap as its first response.
    pub pie_rule: bool,
}

impl TrinityY {
    /// Triangle side length from the node count (3 * side*(side+1)/2).
    fn side_of(board: &BoardGraph) -> usize {
        let per_realm = board.node_count() / 3;
        // side*(side+1)/2 = per_realm
        (((((8 * per_realm + 1) as f64).sqrt() - 1.0) / 2.0).round()) as usize
    }

    /// Who (if anyone) has a Y in this realm: one group touching all
    /// three sides.
    pub fn realm_winner(
        board: &BoardGraph,
        state: &GameState,
        realm_index: usize,
    ) -> Option<Player> {
        let side = Self::side_of(board);
        let per_realm = board.node_count() / 3;
        let lo = (realm_index * per_realm) as NodeId;
        let hi = lo + per_realm as NodeId;
        for player in [Player::Light, Player::Dark] {
            let mut visited = vec![false; per_realm];
            for start in lo..hi {
                if state.occupant(start) != Some(player) || visited[(start - lo) as usize] {
                    continue;
                }
                let mut queue = VecDeque::new();
                visited[(start - lo) as usize] = true;
                queue.push_back(start);
                let mut touch = crate::boardgen::trinity_sides(side, start);
                while let Some(cur) = queue.pop_front() {
                    for &nb in board.neighbors(cur) {
                        if nb < lo || nb >= hi {
                            continue;
                        }
                        if !visited[(nb - lo) as usize] && state.occupant(nb) == Some(player) {
                            visited[(nb - lo) as usize] = true;
                            queue.push_back(nb);
                            touch |= crate::boardgen::trinity_sides(side, nb);
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

    /// Current realm scores ([Light, Dark]).
    pub fn realm_scores(board: &BoardGraph, state: &GameState) -> [u8; 2] {
        let mut scores = [0u8; 2];
        for realm in 0..3 {
            if let Some(p) = Self::realm_winner(board, state, realm) {
                scores[player_index(p)] += 1;
            }
        }
        scores
    }

    fn swap_available(&self, state: &GameState) -> bool {
        self.pie_rule && !state.swap_used && state.ply == 1 && state.to_move == Player::Dark
    }

    /// Realm index of a node.
    fn realm_of(board: &BoardGraph, node: NodeId) -> usize {
        node as usize / (board.node_count() / 3)
    }

    /// Is this realm already won (sealed)?
    fn realm_sealed(board: &BoardGraph, state: &GameState, realm: usize) -> bool {
        // layers tracks realm ownership; recompute is cheap but layers is
        // only a count. Track sealing via winner scan (cached by caller if
        // hot). A realm is sealed iff someone has a Y there.
        Self::realm_winner(board, state, realm).is_some()
    }

    /// The group containing `start` and whether it has any liberty.
    fn group_liberties(
        board: &BoardGraph,
        occ: &[Option<Player>],
        start: NodeId,
    ) -> (Vec<NodeId>, bool) {
        let Some(player) = occ[start as usize] else {
            return (Vec::new(), true); // callers pass occupied starts
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

    /// Apply a placement with Go dynamics onto `occ`. Returns captured
    /// count, or None if the move is suicide.
    fn place_with_capture(
        board: &BoardGraph,
        occ: &mut [Option<Player>],
        sealed: &[bool; 3],
        node: NodeId,
        player: Player,
    ) -> Option<u32> {
        occ[node as usize] = Some(player);
        let mut captured = 0u32;
        // enemy groups adjacent to the new stone, now libertyless, die —
        // but stones in SEALED realms are immortal.
        let realm = Self::realm_of(board, node);
        if !sealed[realm] {
            let mut checked = vec![false; board.node_count()];
            for &nb in board.neighbors(node) {
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
        }
        // suicide check
        let (_, alive) = Self::group_liberties(board, occ, node);
        if !alive {
            return None; // caller restores occ
        }
        Some(captured)
    }

    fn sealed_realms(board: &BoardGraph, state: &GameState) -> [bool; 3] {
        [
            Self::realm_sealed(board, state, 0),
            Self::realm_sealed(board, state, 1),
            Self::realm_sealed(board, state, 2),
        ]
    }
}

impl RuleSet for TrinityY {
    fn id(&self) -> &str {
        TRINITY_Y_V4
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
                let sealed = Self::sealed_realms(board, state);
                if sealed[Self::realm_of(board, *node)] {
                    return Err(RuleError::Occupied(*node)); // sealed realm is closed
                }
                // Go dynamics: suicide + positional superko.
                let mut occ = state.occupancy.clone();
                if Self::place_with_capture(board, &mut occ, &sealed, *node, state.to_move)
                    .is_none()
                {
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
                    // Both stop: realms won decide; tie = draw.
                    let scores = Self::realm_scores(board, &next);
                    next.result = Some(match scores[0].cmp(&scores[1]) {
                        std::cmp::Ordering::Greater => GameResult::Win {
                            player: Player::Light,
                            reason: WinReason::RealmWeave,
                        },
                        std::cmp::Ordering::Less => GameResult::Win {
                            player: Player::Dark,
                            reason: WinReason::RealmWeave,
                        },
                        std::cmp::Ordering::Equal => GameResult::Draw,
                    });
                    return Ok(next);
                }
                next.to_move = mover.opponent();
                return Ok(next);
            }
            Move::Place(node) => {
                let sealed = Self::sealed_realms(board, state);
                // Suicide was rejected by validate_move above.
                let captured =
                    Self::place_with_capture(board, &mut next.occupancy, &sealed, *node, mover)
                        .unwrap_or(0);
                next.captures[player_index(mover)] += captured;
                next.consecutive_passes = 0;
                next.position_hashes.push(position_hash(&next));
            }
            _ => unreachable!("validated"),
        }
        // Track realm scores in `layers` (reuse: [Light, Dark] realms won).
        let scores = Self::realm_scores(board, &next);
        next.layers = scores;
        if scores[player_index(mover)] >= 2 {
            next.result = Some(GameResult::Win {
                player: mover,
                reason: WinReason::RealmWeave,
            });
        } else if scores[player_index(mover.opponent())] >= 2 {
            next.result = Some(GameResult::Win {
                player: mover.opponent(),
                reason: WinReason::RealmWeave,
            });
        }
        next.to_move = mover.opponent();
        Ok(next)
    }

    fn evaluate(&self, _board: &BoardGraph, state: &GameState) -> Option<GameResult> {
        state.result
    }
}
