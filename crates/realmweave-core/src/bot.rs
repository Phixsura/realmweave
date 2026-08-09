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

/// 0/1-BFS: empties crossed to link all origin pairs over the LIVE graph.
fn link_cost(game: &Game, player: Player) -> i64 {
    let bd = game.board();
    let st = game.state();
    let origins = bd.definition().origins_of(player);
    let mut total = 0i64;
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            let (from, to) = (origins[i], origins[j]);
            let n = bd.node_count();
            let mut dist = vec![i64::MAX; n];
            let mut dq = std::collections::VecDeque::new();
            dist[from as usize] = 0;
            dq.push_back(from);
            let mut found = 80;
            while let Some(cur) = dq.pop_front() {
                if cur == to {
                    found = dist[cur as usize];
                    break;
                }
                for nb in bd.live_neighbors(cur, &st.cut_edges) {
                    let cost = match st.occupant(nb) {
                        Some(p) if p == player => 0,
                        None => 1,
                        _ => continue,
                    };
                    let nd = dist[cur as usize] + cost;
                    if nd < dist[nb as usize] {
                        dist[nb as usize] = nd;
                        if cost == 0 {
                            dq.push_front(nb);
                        } else {
                            dq.push_back(nb);
                        }
                    }
                }
            }
            total += found;
        }
    }
    total
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
    theirs - 1.6 * mine + 0.8 * scissor_value
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
        if st.occupant(n).is_none() && near_action[n as usize] {
            out.push(Move::Place(n));
        }
    }
    if out.is_empty() {
        // opening: play near own origins/gates
        for &o in &bd.definition().origins_of(me) {
            for nb in bd.live_neighbors(o, &st.cut_edges) {
                if st.occupant(nb).is_none() {
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
        rules::WEAVE_SEVER_V2 | rules::THREE_REALMS_V1 | rules::SEVER_V1
    )
}
