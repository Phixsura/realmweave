//! Three Realms v1 (classic + sever variant): the original weave race.
//! Superseded as the flagship by trinity-y-v4 but kept as the historical
//! baseline and the tutorial ruleset.

use crate::board::{BoardGraph, NodeId, Player};
use crate::state::{GameResult, GameState, Move, WinReason};

use super::*;

/// Parameterized Three Realms rules. Shared mechanics:
/// - Players alternate turns; stones go on empty nodes; enemy stones block.
/// - Origins are pre-occupied and immovable.
/// - Optional pie rule (swap as Dark's first response; log-only, seats are
///   the caller's concern).
///
/// - A weave = all three own origins connected through the player's own
///   network. Completing it is provisional; it must survive one opponent
///   turn. With sever charges, the response turn has real teeth.
/// - Full board with no confirmed weave: standing provisional weave wins,
///   otherwise draw.
pub struct WeaveRules {
    id: &'static str,
    /// Whether Dark may swap as its first response.
    pub pie_rule: bool,
    sever_charges: u8,
}

impl WeaveRules {
    /// three-realms-v1: the original weave race.
    pub fn classic(pie_rule: bool) -> Self {
        WeaveRules {
            id: THREE_REALMS_V1,
            pie_rule,
            sever_charges: 0,
        }
    }

    /// three-realms-sever-v1: weave race + stone-removal charges.
    pub fn sever(pie_rule: bool) -> Self {
        WeaveRules {
            id: SEVER_V1,
            pie_rule,
            sever_charges: SEVER_CHARGES,
        }
    }

    fn swap_available(&self, state: &GameState) -> bool {
        self.pie_rule && !state.swap_used && state.ply == 1 && state.to_move == Player::Dark
    }

    fn board_full(state: &GameState) -> bool {
        state.occupancy.iter().all(|occ| occ.is_some())
    }

    fn has_weave(&self, board: &BoardGraph, state: &GameState, player: Player) -> bool {
        has_realm_weave(board, state, player)
    }

    fn charges(state: &GameState, player: Player) -> u8 {
        state.sever_charges[player_index(player)]
    }
}

impl RuleSet for WeaveRules {
    fn id(&self) -> &str {
        self.id
    }

    fn setup(&self, state: &mut GameState) {
        state.sever_charges = [self.sever_charges, self.sever_charges];
    }

    fn legal_moves(&self, board: &BoardGraph, state: &GameState) -> Vec<Move> {
        if state.is_finished() {
            return Vec::new();
        }
        let mut moves: Vec<Move> = (0..board.node_count() as NodeId)
            .filter(|&n| state.occupant(n).is_none())
            .map(Move::Place)
            .collect();
        if Self::charges(state, state.to_move) > 0 {
            let enemy = state.to_move.opponent();
            let origins: Vec<NodeId> = board.definition().origins_of(enemy);
            moves.extend(
                (0..board.node_count() as NodeId)
                    .filter(|&n| state.occupant(n) == Some(enemy) && !origins.contains(&n))
                    .map(Move::Sever),
            );
        }
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
                Ok(())
            }
            Move::Sever(node) => {
                if Self::charges(state, state.to_move) == 0 {
                    return Err(RuleError::SeverUnavailable);
                }
                if *node as usize >= board.node_count() {
                    return Err(RuleError::NoSuchNode(*node));
                }
                let enemy = state.to_move.opponent();
                if state.occupant(*node) != Some(enemy) {
                    return Err(RuleError::CannotSever(*node));
                }
                if board.definition().origins_of(enemy).contains(node) {
                    return Err(RuleError::CannotSever(*node));
                }
                Ok(())
            }
            Move::CutEdge(e) => Err(RuleError::CannotCut(*e)),
            Move::Pass => Err(RuleError::PassUnavailable),
            Move::Swap => {
                if self.swap_available(state) {
                    Ok(())
                } else {
                    Err(RuleError::SwapUnavailable)
                }
            }
            Move::Resign => Ok(()),
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
            Move::Swap => {
                // Colors are unchanged; seats swap outside the engine.
                return Ok(next);
            }
            Move::Pass => return Err(RuleError::PassUnavailable),
            Move::CutEdge(e) => return Err(RuleError::CannotCut(*e)),
            Move::Place(node) => {
                next.occupancy[*node as usize] = Some(mover);
                next.consecutive_passes = 0;
            }
            Move::Sever(node) => {
                next.occupancy[*node as usize] = None;
                next.sever_charges[player_index(mover)] -= 1;
                next.consecutive_passes = 0;
            }
        }

        // Weave confirmation: if the opponent had a provisional weave, this
        // move was the response turn. (A sever may have just broken it.)
        if next.pending_weave == Some(mover.opponent()) {
            if self.has_weave(board, &next, mover.opponent()) {
                next.result = Some(GameResult::Win {
                    player: mover.opponent(),
                    reason: WinReason::RealmWeave,
                });
                return Ok(next);
            }
            next.pending_weave = None;
        }

        // New provisional weave by the mover?
        if self.has_weave(board, &next, mover) {
            next.pending_weave = Some(mover);
        } else if next.pending_weave == Some(mover) {
            next.pending_weave = None;
        }

        if Self::board_full(&next) && next.result.is_none() {
            // No confirmation turn is possible on a full board: a standing
            // weave that the opponent can never answer wins outright.
            // (With sever charges the opponent could still answer, but a
            // full board with charges left is not reachable in practice and
            // the simple rule keeps termination guaranteed.)
            if next.pending_weave == Some(mover) {
                next.result = Some(GameResult::Win {
                    player: mover,
                    reason: WinReason::RealmWeave,
                });
            } else {
                next.result = Some(GameResult::Draw);
            }
            return Ok(next);
        }

        next.to_move = mover.opponent();
        Ok(next)
    }

    fn evaluate(&self, _board: &BoardGraph, state: &GameState) -> Option<GameResult> {
        state.result
    }
}
