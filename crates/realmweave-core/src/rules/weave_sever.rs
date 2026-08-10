//! Weave & Sever v2 / Weave Layers v3: scissors edge-cutting, strangles,
//! petrifying layer scoring, fossil roads. Docs: design-weave-sever-v2.md,
//! design-weave-layers-v3.md.

use std::collections::VecDeque;

use crate::board::{BoardGraph, NodeId, Player};
use crate::state::{GameResult, GameState, Move, WinReason};

use super::*;

// ------------------------------------------------------------ weave-sever ---

/// Weave & Sever v2 — the design in docs/design-weave-sever-v2.md.
///
/// Each turn: Place a stone, Cut an edge (K scissors each, origin-adjacent
/// edges protected), or Pass. Win by confirmed Realm Weave over the living
/// graph, or instantly by strangling: the opponent's origins can never be
/// connected again even given every empty node. Self-strangling cuts are
/// illegal. Fallback scoring if the game stalls (two passes / full board):
/// most potentially-connectable origins, then most scissors, then draw.
pub struct WeaveSeverV2 {
    /// Whether Dark may swap as its first response.
    pub pie_rule: bool,
    /// 1 = classic v2 (first confirmed weave wins). >1 = weave-layers-v3:
    /// each confirmed weave scores a layer and petrifies its network; first
    /// to this many layers wins.
    pub layers_to_win: u8,
}

impl WeaveSeverV2 {
    fn swap_available(&self, state: &GameState) -> bool {
        self.pie_rule && !state.swap_used && state.ply == 1 && state.to_move == Player::Dark
    }

    /// Sanctum radius for this mode: wider on larger boards; a radius-2
    /// halo would swallow most of tiny hex19.
    fn sanctum_radius(&self, board: &BoardGraph) -> u32 {
        if self.layers_to_win > 1 && board.node_count() >= 37 * 3 {
            2
        } else {
            1
        }
    }

    /// Nodes within graph distance `radius` of any origin (any player).
    fn origin_zone(board: &BoardGraph, radius: u32) -> Vec<bool> {
        let def = board.definition();
        let mut zone = vec![false; board.node_count()];
        let mut queue = VecDeque::new();
        let mut dist = vec![u32::MAX; board.node_count()];
        for o in &def.origins {
            dist[o.node as usize] = 0;
            queue.push_back(o.node);
        }
        while let Some(cur) = queue.pop_front() {
            if dist[cur as usize] >= radius {
                continue;
            }
            for &nb in board.neighbors(cur) {
                if dist[nb as usize] == u32::MAX {
                    dist[nb as usize] = dist[cur as usize] + 1;
                    queue.push_back(nb);
                }
            }
        }
        for (n, d) in dist.iter().enumerate() {
            if *d <= radius {
                zone[n] = true;
            }
        }
        zone
    }

    /// Can edge `e` be cut at all (regardless of whose turn)?
    fn edge_cuttable(
        &self,
        board: &BoardGraph,
        state: &GameState,
        e: u32,
    ) -> Result<(), RuleError> {
        let def = board.definition();
        let Some(edge) = def.edges.get(e as usize) else {
            return Err(RuleError::CannotCut(e));
        };
        if state.cut_edges.contains(&e) {
            return Err(RuleError::CannotCut(e));
        }
        // Origin-adjacent edges (either player's) are protected. In the
        // layers game the protected halo widens to radius 1 around every
        // origin, and PORTAL edges are the world's skeleton — uncuttable.
        // Gates are fought over by occupation, not demolition; scissors
        // shape the terrain within realms.
        if self.layers_to_win > 1 {
            if edge.kind == crate::board::EdgeKind::Portal {
                return Err(RuleError::CannotCut(e));
            }
            let zone = Self::origin_zone(board, 1);
            if zone[edge.a as usize] || zone[edge.b as usize] {
                return Err(RuleError::CannotCut(e));
            }
        } else {
            let is_origin = |n: NodeId| def.origins.iter().any(|o| o.node == n);
            if is_origin(edge.a) || is_origin(edge.b) {
                return Err(RuleError::CannotCut(e));
            }
        }
        // Petrified world structure cannot be re-carved (v3).
        if state.is_petrified(edge.a) || state.is_petrified(edge.b) {
            return Err(RuleError::CannotCut(e));
        }
        Ok(())
    }

    /// v3: petrify `player`'s weave network. The network = the connected
    /// component (over live edges, own stones + origins) containing the
    /// origins. Origin-adjacent stones are REMOVED instead of petrified —
    /// origins must keep breathing room (design §1.2).
    fn petrify_weave(board: &BoardGraph, state: &mut GameState, player: Player) {
        let def = board.definition();
        let origins = def.origins_of(player);
        // Collect the network via BFS over own stones from the origins.
        let mut in_net = vec![false; board.node_count()];
        let mut queue = VecDeque::new();
        for &o in &origins {
            if !in_net[o as usize] {
                in_net[o as usize] = true;
                queue.push_back(o);
            }
        }
        while let Some(cur) = queue.pop_front() {
            for nb in board.live_neighbors(cur, &state.cut_edges) {
                if !in_net[nb as usize] && state.occupant(nb) == Some(player) {
                    in_net[nb as usize] = true;
                    queue.push_back(nb);
                }
            }
        }
        if state.petrified.len() < board.node_count() {
            state.petrified.resize(board.node_count(), None);
        }
        let origin_adjacent: Vec<bool> = {
            let mut adj = vec![false; board.node_count()];
            for o in &def.origins {
                for &nb in board.neighbors(o.node) {
                    adj[nb as usize] = true;
                }
            }
            adj
        };
        let is_origin: Vec<bool> = {
            let mut v = vec![false; board.node_count()];
            for o in &def.origins {
                v[o.node as usize] = true;
            }
            v
        };
        for n in 0..board.node_count() {
            if !in_net[n] || is_origin[n] || state.occupant(n as NodeId) != Some(player) {
                continue; // origins themselves stay origins
            }
            state.occupancy[n] = None;
            if !origin_adjacent[n] {
                state.petrified[n] = Some(player);
            }
            // origin-adjacent: removed entirely (breathing room)
        }
        // Layer scissors for both sides, capped.
        for sc in state.scissors.iter_mut() {
            *sc = (*sc + LAYER_SCISSORS).min(SCISSORS_CAP);
        }
    }

    /// Doom check for this mode: v2 = potential connectivity (stones
    /// block); v3 = permanent terrain only (cuts + petrification).
    fn doomed(&self, board: &BoardGraph, state: &GameState, player: Player) -> bool {
        if self.layers_to_win > 1 {
            !permanently_connected(board, state, player)
        } else {
            !potential_connected(board, state, player)
        }
    }

    fn board_full(state: &GameState) -> bool {
        state
            .occupancy
            .iter()
            .enumerate()
            .all(|(i, occ)| occ.is_some() || state.is_petrified(i as NodeId))
    }

    /// Fallback scoring per §2.5 of the design (v3: layers decide first).
    fn fallback_result(board: &BoardGraph, state: &GameState) -> GameResult {
        match state.layers[0].cmp(&state.layers[1]) {
            std::cmp::Ordering::Greater => {
                return GameResult::Win {
                    player: Player::Light,
                    reason: WinReason::RealmWeave,
                }
            }
            std::cmp::Ordering::Less => {
                return GameResult::Win {
                    player: Player::Dark,
                    reason: WinReason::RealmWeave,
                }
            }
            std::cmp::Ordering::Equal => {}
        }
        let l = potential_origin_groups(board, state, Player::Light);
        let d = potential_origin_groups(board, state, Player::Dark);
        match l.cmp(&d) {
            std::cmp::Ordering::Greater => GameResult::Win {
                player: Player::Light,
                reason: WinReason::Strangle,
            },
            std::cmp::Ordering::Less => GameResult::Win {
                player: Player::Dark,
                reason: WinReason::Strangle,
            },
            std::cmp::Ordering::Equal => match state.scissors[0].cmp(&state.scissors[1]) {
                std::cmp::Ordering::Greater => GameResult::Win {
                    player: Player::Light,
                    reason: WinReason::Strangle,
                },
                std::cmp::Ordering::Less => GameResult::Win {
                    player: Player::Dark,
                    reason: WinReason::Strangle,
                },
                std::cmp::Ordering::Equal => GameResult::Draw,
            },
        }
    }
}

impl RuleSet for WeaveSeverV2 {
    fn id(&self) -> &str {
        if self.layers_to_win > 1 {
            WEAVE_LAYERS_V3
        } else {
            WEAVE_SEVER_V2
        }
    }

    fn setup(&self, state: &mut GameState) {
        state.scissors = [SCISSORS, SCISSORS];
    }

    fn legal_moves(&self, board: &BoardGraph, state: &GameState) -> Vec<Move> {
        if state.is_finished() {
            return Vec::new();
        }
        // Origin sanctum: nodes near an ENEMY origin are unplaceable
        // (radius 1 in v2, radius 2 in the layers game).
        let def = board.definition();
        let enemy = state.to_move.opponent();
        let radius = self.sanctum_radius(board);
        let mut sanctum = vec![false; board.node_count()];
        for o in def.origins.iter().filter(|o| o.player == enemy) {
            let mut dist = vec![u32::MAX; board.node_count()];
            let mut queue = VecDeque::new();
            dist[o.node as usize] = 0;
            queue.push_back(o.node);
            while let Some(cur) = queue.pop_front() {
                if dist[cur as usize] >= radius {
                    continue;
                }
                for &nb in board.neighbors(cur) {
                    if dist[nb as usize] == u32::MAX {
                        dist[nb as usize] = dist[cur as usize] + 1;
                        sanctum[nb as usize] = true;
                        queue.push_back(nb);
                    }
                }
            }
        }
        let mut moves: Vec<Move> = (0..board.node_count() as NodeId)
            .filter(|&n| {
                state.occupant(n).is_none() && !sanctum[n as usize] && !state.is_petrified(n)
            })
            .map(Move::Place)
            .collect();
        if state.scissors[player_index(state.to_move)] > 0 {
            for e in 0..board.definition().edges.len() as u32 {
                if self.edge_cuttable(board, state, e).is_ok()
                    && !cut_self_strangles(board, state, state.to_move, e, self.layers_to_win > 1)
                {
                    moves.push(Move::CutEdge(e));
                }
            }
        }
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
                if state.occupant(*node).is_some() || state.is_petrified(*node) {
                    return Err(RuleError::Occupied(*node));
                }
                // Origin sanctum: you may not place near an ENEMY origin
                // (radius 1 in v2, radius 2 in the layers game). Origins
                // are low-degree corner nodes; without this a small
                // blockade strangles them outright and every game
                // degenerates into a blockade race.
                let def = board.definition();
                let enemy = state.to_move.opponent();
                let radius = self.sanctum_radius(board);
                for o in def.origins.iter().filter(|o| o.player == enemy) {
                    // BFS out to `radius` from the origin.
                    let mut dist = vec![u32::MAX; board.node_count()];
                    let mut queue = VecDeque::new();
                    dist[o.node as usize] = 0;
                    queue.push_back(o.node);
                    while let Some(cur) = queue.pop_front() {
                        if cur == *node && dist[cur as usize] <= radius {
                            return Err(RuleError::OriginSanctum(*node));
                        }
                        if dist[cur as usize] >= radius {
                            continue;
                        }
                        for &nb in board.neighbors(cur) {
                            if dist[nb as usize] == u32::MAX {
                                dist[nb as usize] = dist[cur as usize] + 1;
                                queue.push_back(nb);
                            }
                        }
                    }
                }
                Ok(())
            }
            Move::CutEdge(e) => {
                if state.scissors[player_index(state.to_move)] == 0 {
                    return Err(RuleError::NoScissors);
                }
                self.edge_cuttable(board, state, *e)?;
                if cut_self_strangles(board, state, state.to_move, *e, self.layers_to_win > 1) {
                    return Err(RuleError::SelfStrangle);
                }
                Ok(())
            }
            Move::Pass => Ok(()),
            Move::Swap => {
                if self.swap_available(state) {
                    Ok(())
                } else {
                    Err(RuleError::SwapUnavailable)
                }
            }
            Move::Sever(n) => Err(RuleError::CannotSever(*n)),
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
        let opp = mover.opponent();
        let mut next = state.clone();
        next.move_log.push(*mv);
        next.ply += 1;
        if next.ply >= 2 {
            next.swap_used = true;
        }

        match mv {
            Move::Resign => {
                next.result = Some(GameResult::Win {
                    player: opp,
                    reason: WinReason::Resignation,
                });
                return Ok(next);
            }
            Move::Swap => return Ok(next),
            Move::Pass => {
                next.consecutive_passes += 1;
                if next.consecutive_passes >= 2 {
                    next.result = Some(Self::fallback_result(board, &next));
                    return Ok(next);
                }
                // A pass forfeits the pending-weave response; confirmation
                // still runs below.
            }
            Move::Place(node) => {
                next.occupancy[*node as usize] = Some(mover);
                next.consecutive_passes = 0;
            }
            Move::CutEdge(e) => {
                next.cut_edges.push(*e);
                next.scissors[player_index(mover)] -= 1;
                next.consecutive_passes = 0;
            }
            Move::Sever(n) => return Err(RuleError::CannotSever(*n)),
        }

        // 1. Strangle check — death before life (design §2.3, edge case 4).
        //    A cut or a blocking placement may have doomed the opponent.
        if self.doomed(board, &next, opp) {
            next.result = Some(GameResult::Win {
                player: mover,
                reason: WinReason::Strangle,
            });
            return Ok(next);
        }

        // 2. Pending-weave confirmation (opponent's weave survived my turn?).
        if next.pending_weave == Some(opp) {
            if live_realm_weave(board, &next, opp) {
                next.layers[player_index(opp)] += 1;
                if next.layers[player_index(opp)] >= self.layers_to_win {
                    next.result = Some(GameResult::Win {
                        player: opp,
                        reason: WinReason::RealmWeave,
                    });
                    return Ok(next);
                }
                // v3: the confirmed weave petrifies into world structure
                // and play continues on the transformed board.
                Self::petrify_weave(board, &mut next, opp);
                next.pending_weave = None;
                // Petrification may have doomed someone: strangle checks.
                // Self-doom by petrify loses (design edge case 2); check
                // the petrifier FIRST so simultaneous doom favors the
                // non-scorer per that rule.
                if self.doomed(board, &next, opp) {
                    next.result = Some(GameResult::Win {
                        player: mover,
                        reason: WinReason::Strangle,
                    });
                    return Ok(next);
                }
                if self.doomed(board, &next, mover) {
                    next.result = Some(GameResult::Win {
                        player: opp,
                        reason: WinReason::Strangle,
                    });
                    return Ok(next);
                }
            } else {
                next.pending_weave = None;
            }
        }

        // 3. New provisional weave by the mover?
        if live_realm_weave(board, &next, mover) {
            next.pending_weave = Some(mover);
        } else if next.pending_weave == Some(mover) {
            next.pending_weave = None;
        }

        // 4a. Ply cap (v3): close the game and score it.
        if self.layers_to_win > 1 && next.ply >= V3_PLY_CAP && next.result.is_none() {
            next.result = Some(Self::fallback_result(board, &next));
            return Ok(next);
        }

        // 4. Full-board endgame (no response turn possible).
        if Self::board_full(&next) && next.result.is_none() {
            if next.pending_weave == Some(mover) {
                // A standing weave the opponent can never answer scores.
                next.layers[player_index(mover)] += 1;
            }
            next.result = Some(if next.layers[player_index(mover)] >= self.layers_to_win {
                GameResult::Win {
                    player: mover,
                    reason: WinReason::RealmWeave,
                }
            } else {
                Self::fallback_result(board, &next)
            });
            return Ok(next);
        }

        next.to_move = opp;
        Ok(next)
    }

    fn evaluate(&self, _board: &BoardGraph, state: &GameState) -> Option<GameResult> {
        state.result
    }

    fn uses_scissors(&self) -> bool {
        true
    }

    fn allows_pass(&self) -> bool {
        true
    }
}
