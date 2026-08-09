//! A playable opponent for weave-sever-v2 (and classic).
//!
//! 2-ply minimax over a connection-based evaluation with move filtering.
//! Not a strong engine — a *sparring partner*: it builds toward its weave,
//! blocks yours when cheap, cuts your bridges at the right moments, and
//! never plays illegal or pointless moves. Deterministic given (state,
//! seed) so games remain replayable.

use crate::board::BoardGraph;
use crate::rules;
use crate::state::{GameResult, Move};
use crate::{Game, NodeId, Player};

/// Deterministic tie-break noise.
fn noise(seed: u64, ply: u32, k: usize) -> f64 {
    let mut x = seed
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(ply as u64)
        .wrapping_mul(0xBF58476D1CE4E5B9)
        .wrapping_add(k as u64 + 1);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 29;
    (x as f64) / (u64::MAX as f64)
}

/// Per-node entry cost for `player` route-finding. Contested ground —
/// empties adjacent to enemy stones — is more expensive: routes that hug
/// the opponent are fragile (one enemy stone or cut closes them), so a
/// careful player detours or reinforces. This single term is what bends
/// play from straight lines into Go-like shapes.
fn entry_cost(game: &Game, player: Player, node: NodeId) -> Option<u32> {
    let st = game.state();
    if st.is_petrified(node) {
        return None; // world structure: impassable
    }
    match st.occupant(node) {
        Some(p) if p == player => Some(0),
        Some(_) => None,
        None => {
            let bd = game.board();
            let enemy_adj = bd
                .live_neighbors(node, &st.cut_edges)
                .filter(|&nb| st.occupant(nb) == Some(player.opponent()))
                .count() as u32;
            // base 2 per empty; +1 per adjacent enemy stone (cap +3)
            Some(2 + enemy_adj.min(3))
        }
    }
}

/// Dijkstra from `from` to `to` over live edges with contested-ground
/// costs. Returns (cost, path-nodes) or None if unreachable.
fn route(game: &Game, player: Player, from: NodeId, to: NodeId) -> Option<(i64, Vec<NodeId>)> {
    let bd = game.board();
    let st = game.state();
    let n = bd.node_count();
    let mut dist = vec![i64::MAX; n];
    let mut prev = vec![NodeId::MAX; n];
    let mut heap = std::collections::BinaryHeap::new();
    dist[from as usize] = 0;
    heap.push(std::cmp::Reverse((0i64, from)));
    while let Some(std::cmp::Reverse((d, cur))) = heap.pop() {
        if cur == to {
            let mut path = vec![to];
            let mut c = to;
            while prev[c as usize] != NodeId::MAX {
                c = prev[c as usize];
                path.push(c);
            }
            path.reverse();
            return Some((d, path));
        }
        if d > dist[cur as usize] {
            continue;
        }
        for nb in bd.live_neighbors(cur, &st.cut_edges) {
            let Some(cost) = entry_cost(game, player, nb) else {
                continue;
            };
            let nd = d + cost as i64;
            if nd < dist[nb as usize] {
                dist[nb as usize] = nd;
                prev[nb as usize] = cur;
                heap.push(std::cmp::Reverse((nd, nb)));
            }
        }
    }
    None
}

/// Redundancy-aware link cost for one origin pair: cheapest route, plus
/// half the cost of the best *alternative* route that avoids the first
/// route's empty nodes. A single thin line scores much worse than a web —
/// this is what makes the bot build shapes instead of marching.
fn pair_cost(game: &Game, player: Player, from: NodeId, to: NodeId) -> i64 {
    const UNREACHABLE: i64 = 240;
    let Some((best, path)) = route(game, player, from, to) else {
        return UNREACHABLE;
    };
    // Second path: forbid the first path's empty interior nodes.
    let st = game.state();
    let blocked: Vec<NodeId> = path
        .iter()
        .copied()
        .filter(|&nd| st.occupant(nd).is_none() && nd != from && nd != to)
        .collect();
    let alt = route_avoiding(game, player, from, to, &blocked).map(|(c, _)| c);
    // If no alternative exists the connection hangs by a thread.
    best + alt.unwrap_or(UNREACHABLE / 2) / 2
}

fn route_avoiding(
    game: &Game,
    player: Player,
    from: NodeId,
    to: NodeId,
    blocked: &[NodeId],
) -> Option<(i64, Vec<NodeId>)> {
    let bd = game.board();
    let st = game.state();
    let n = bd.node_count();
    let mut dist = vec![i64::MAX; n];
    let mut prev = vec![NodeId::MAX; n];
    let mut heap = std::collections::BinaryHeap::new();
    dist[from as usize] = 0;
    heap.push(std::cmp::Reverse((0i64, from)));
    while let Some(std::cmp::Reverse((d, cur))) = heap.pop() {
        if cur == to {
            let mut path = vec![to];
            let mut c = to;
            while prev[c as usize] != NodeId::MAX {
                c = prev[c as usize];
                path.push(c);
            }
            path.reverse();
            return Some((d, path));
        }
        if d > dist[cur as usize] {
            continue;
        }
        for nb in bd.live_neighbors(cur, &st.cut_edges) {
            if blocked.contains(&nb) {
                continue;
            }
            let Some(cost) = entry_cost(game, player, nb) else {
                continue;
            };
            let nd = d + cost as i64;
            if nd < dist[nb as usize] {
                dist[nb as usize] = nd;
                prev[nb as usize] = cur;
                heap.push(std::cmp::Reverse((nd, nb)));
            }
        }
    }
    None
}

/// Redundancy-aware cost to link all origin pairs (lower = healthier).
/// Public so UIs can narrate how a move changed each side's position.
pub fn link_cost(game: &Game, player: Player) -> i64 {
    let bd = game.board();
    let origins = bd.definition().origins_of(player);
    let mut total = 0i64;
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            total += pair_cost(game, player, origins[i], origins[j]);
        }
    }
    total
}

/// The current cheapest route nodes for `player` (all origin pairs),
/// for UI visualization of intentions.
pub fn best_routes(game: &Game, player: Player) -> Vec<NodeId> {
    let bd = game.board();
    let origins = bd.definition().origins_of(player);
    let mut out = Vec::new();
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            if let Some((_, path)) = route(game, player, origins[i], origins[j]) {
                out.extend(path);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Static evaluation from `me`'s perspective: lower own link cost is good,
/// higher opponent cost is good; scissors are worth keeping.
fn evaluate(game: &Game, me: Player) -> f64 {
    if let Some(result) = game.result() {
        return match result {
            GameResult::Win { player, .. } if player == me => 10_000.0,
            GameResult::Win { .. } => -10_000.0,
            GameResult::Draw => 0.0,
        };
    }
    let mine = link_cost(game, me) as f64;
    let theirs = link_cost(game, me.opponent()) as f64;
    let scissor_value = game.state().scissors[match me {
        Player::Light => 0,
        Player::Dark => 1,
    }] as f64;
    let layer_lead = game.state().layers[match me {
        Player::Light => 0,
        Player::Dark => 1,
    }] as f64
        - game.state().layers[match me {
            Player::Light => 1,
            Player::Dark => 0,
        }] as f64;
    // Defense-weighted: protecting your own web matters more than hurting
    // theirs — aggression-first weights degenerate into strangle races.
    theirs - 2.4 * mine + 1.2 * scissor_value + 900.0 * layer_lead
}

/// Candidate moves worth considering (keeps branching manageable):
/// placements near the action + cuts on the opponent's likely paths.
fn candidates(game: &Game, me: Player) -> Vec<Move> {
    let bd = game.board();
    let st = game.state();
    let mut out = Vec::new();

    // Placements: empty nodes adjacent to any stone, or near origins.
    let mut near_action = vec![false; bd.node_count()];
    for n in 0..bd.node_count() as NodeId {
        if st.occupant(n).is_some() {
            near_action[n as usize] = true;
            for nb in bd.live_neighbors(n, &st.cut_edges) {
                near_action[nb as usize] = true;
            }
        }
    }
    for n in 0..bd.node_count() as NodeId {
        if st.occupant(n).is_none() && !st.is_petrified(n) && near_action[n as usize] {
            out.push(Move::Place(n));
        }
    }
    if out.is_empty() {
        // opening: play near own origins/gates
        for &o in &bd.definition().origins_of(me) {
            for nb in bd.live_neighbors(o, &st.cut_edges) {
                if st.occupant(nb).is_none() && !st.is_petrified(nb) {
                    out.push(Move::Place(nb));
                }
            }
        }
    }

    // Cuts: only when we have scissors — target edges between/next to
    // opponent stones (their bridges) or on their cheapest path.
    let my_scissors = st.scissors[match me {
        Player::Light => 0,
        Player::Dark => 1,
    }];
    if my_scissors > 0 {
        let opp = me.opponent();
        for (ei, edge) in bd.definition().edges.iter().enumerate() {
            let e = ei as u32;
            if st.cut_edges.contains(&e) {
                continue;
            }
            let a_opp = st.occupant(edge.a) == Some(opp);
            let b_opp = st.occupant(edge.b) == Some(opp);
            // any edge touching an enemy stone is a cut candidate
            if a_opp || b_opp {
                out.push(Move::CutEdge(e));
            }
        }
    }
    out
}

/// Choose the bot's move: 2-ply search (my move → opponent's best reply by
/// static eval) over filtered candidates.
pub fn choose_move(game: &Game, seed: u64) -> Option<Move> {
    let me = game.to_move();
    let cands: Vec<Move> = candidates(game, me)
        .into_iter()
        .filter(|m| game.validate(m).is_ok())
        .collect();
    if cands.is_empty() {
        return game
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m, Move::Place(_) | Move::Pass));
    }
    let ply = game.state().ply;

    // Pass 1: static score, keep top 12.
    let mut scored: Vec<(f64, Move)> = Vec::new();
    for (k, &mv) in cands.iter().enumerate() {
        let bd = BoardGraph::new(game.board().definition().clone()).ok()?;
        let mut sim = Game::replay(bd, game.config().clone(), &game.state().move_log).ok()?;
        if sim.play(mv).is_err() {
            continue;
        }
        let s = evaluate(&sim, me) + noise(seed, ply, k) * 0.3;
        scored.push((s, mv));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(12);

    // Pass 2: opponent's best static reply (from THEIR candidate set).
    let mut best: Option<(f64, Move)> = None;
    for &(_, mv) in &scored {
        let bd = BoardGraph::new(game.board().definition().clone()).ok()?;
        let mut sim = Game::replay(bd, game.config().clone(), &game.state().move_log).ok()?;
        if sim.play(mv).is_err() {
            continue;
        }
        let after = if sim.result().is_some() {
            evaluate(&sim, me)
        } else {
            // opponent minimizes my eval
            let opp = me.opponent();
            let mut worst = f64::INFINITY;
            let reply_cands: Vec<Move> = candidates(&sim, opp)
                .into_iter()
                .filter(|m| sim.validate(m).is_ok())
                .take(16)
                .collect();
            if reply_cands.is_empty() {
                evaluate(&sim, me)
            } else {
                for rmv in reply_cands {
                    let bd2 = BoardGraph::new(sim.board().definition().clone()).ok()?;
                    let mut sim2 =
                        Game::replay(bd2, sim.config().clone(), &sim.state().move_log).ok()?;
                    if sim2.play(rmv).is_err() {
                        continue;
                    }
                    let v = evaluate(&sim2, me);
                    if v < worst {
                        worst = v;
                    }
                }
                if worst.is_finite() {
                    worst
                } else {
                    evaluate(&sim, me)
                }
            }
        };
        if best.map(|(b, _)| after > b).unwrap_or(true) {
            best = Some((after, mv));
        }
    }
    best.map(|(_, m)| m)
        .or_else(|| scored.first().map(|(_, m)| *m))
}

/// Convenience: does this game's ruleset support the bot?
pub fn supports(ruleset_id: &str) -> bool {
    matches!(
        ruleset_id,
        rules::WEAVE_SEVER_V2 | rules::WEAVE_LAYERS_V3 | rules::THREE_REALMS_V1 | rules::SEVER_V1
    )
}
