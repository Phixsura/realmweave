//! Territory-aware bot: a hand-tuned multi-factor evaluator in the spirit
//! of classical (pre-MCTS) Go programs.
//!
//! For each candidate move it plays the move on a scratch game and scores
//! the resulting position with:
//!
//! - **area**: stones + exclusive territory (the actual win condition)
//! - **influence**: soft ownership of contested empty nodes by distance
//! - **group safety**: penalty for own groups with thin supply margins,
//!   bonus for enemy groups near starvation (attack!)
//! - **connection**: origin-linking progress (weave bonus is 10 points)
//! - **captures**: immediate material from the move
//!
//! It searches only 1 ply + a cheap opponent-best-reply check on the top
//! candidates (2-ply "quiescence"), which is enough to stop the
//! straight-line racing style of the greedy bot: this bot fights for
//! borders, defends weak groups, and invades open areas.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use realmweave_core::{rules, supply_score, BoardGraph, Game, GameConfig, Move, NodeId, Player};

use crate::bots::Bot;

pub struct TerritoryBot {
    pub rng: StdRng,
    /// How many top candidates get the opponent-reply check.
    pub reply_width: usize,
}

impl Default for TerritoryBot {
    fn default() -> Self {
        TerritoryBot {
            rng: rand::SeedableRng::seed_from_u64(0),
            reply_width: 8,
        }
    }
}

/// Rebuild an owned copy of a game (Game holds a boxed ruleset).
fn clone_game(game: &Game) -> Game {
    let board = BoardGraph::new(game.board().definition().clone()).expect("board");
    let config: GameConfig = game.config().clone();
    Game::replay(board, config, &game.state().move_log).expect("replay clone")
}

/// Static evaluation of a position from `me`'s perspective (higher = better).
fn evaluate(game: &Game, me: Player) -> f64 {
    let board = game.board();
    let state = game.state();
    let opp = me.opponent();

    // --- area: the real scoreboard ---
    let my_score = supply_score(board, state, me);
    let their_score = supply_score(board, state, opp);
    let area = (my_score.total_half() - their_score.total_half()) as f64 / 2.0;

    // --- influence: distance-weighted soft ownership of empty nodes ---
    let my_dist = multi_source_distance(game, me);
    let their_dist = multi_source_distance(game, opp);
    let mut influence = 0.0;
    for n in 0..board.node_count() {
        if state.occupancy[n].is_some() {
            continue;
        }
        match (my_dist[n], their_dist[n]) {
            (Some(a), Some(b)) => {
                if a + 1 < b {
                    influence += 0.35;
                } else if b + 1 < a {
                    influence -= 0.35;
                }
            }
            (Some(_), None) => influence += 0.5,
            (None, Some(_)) => influence -= 0.5,
            (None, None) => {}
        }
    }

    // --- group safety: supply margin per group ---
    // A group whose supply would die if `margin` frontier empties flipped is
    // fragile. Approximate margin with the count of distinct empty
    // neighbors on the group's shortest supply frontier: cheap proxy =
    // number of empty neighbors of the group (liberties analog).
    let mut safety = 0.0;
    for (player, sign) in [(me, 1.0), (opp, -1.0)] {
        for group in rules::player_components(board, state, player) {
            let has_origin = board
                .definition()
                .origins_of(player)
                .iter()
                .any(|o| group.binary_search(o).is_ok());
            if has_origin {
                continue; // origin groups never die
            }
            let mut liberties = std::collections::HashSet::new();
            for &g in &group {
                for &nb in board.neighbors(g) {
                    if state.occupant(nb).is_none() {
                        liberties.insert(nb);
                    }
                }
            }
            let libs = liberties.len() as f64;
            let size = group.len() as f64;
            // Thin groups are liabilities proportional to their size.
            let danger = match libs as u32 {
                0 => size,       // dead (shouldn't happen post-capture)
                1 => size * 0.9, // one move from death
                2 => size * 0.5, // cuttable
                3 => size * 0.2,
                _ => 0.0,
            };
            safety -= sign * danger; // my danger lowers my eval
        }
    }

    // --- weave progress: distance to linking all origins ---
    let my_link = origin_link_cost(game, me);
    let their_link = origin_link_cost(game, opp);
    let weave_progress = (their_link - my_link) as f64 * 0.25;

    area * 2.0 + influence + safety * 1.5 + weave_progress
}

/// Multi-source BFS distance from all of `player`'s stones through empty
/// nodes (enemy stones block). None = unreachable.
fn multi_source_distance(game: &Game, player: Player) -> Vec<Option<u32>> {
    let board = game.board();
    let state = game.state();
    let n = board.node_count();
    let mut dist = vec![None; n];
    let mut queue = std::collections::VecDeque::new();
    for node in 0..n as NodeId {
        if state.occupant(node) == Some(player) {
            dist[node as usize] = Some(0);
            queue.push_back(node);
        }
    }
    while let Some(cur) = queue.pop_front() {
        let d = dist[cur as usize].unwrap();
        for &next in board.neighbors(cur) {
            if state.occupant(next).is_none() && dist[next as usize].is_none() {
                dist[next as usize] = Some(d + 1);
                queue.push_back(next);
            }
        }
    }
    dist
}

/// 0/1-BFS cost (empties crossed) to connect all origin pairs.
fn origin_link_cost(game: &Game, player: Player) -> i64 {
    let board = game.board();
    let origins = board.definition().origins_of(player);
    let mut total = 0i64;
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            total += zero_one(game, origins[i], origins[j], player).unwrap_or(60);
        }
    }
    total
}

fn zero_one(game: &Game, from: NodeId, to: NodeId, player: Player) -> Option<i64> {
    let board = game.board();
    let state = game.state();
    let n = board.node_count();
    let mut dist = vec![i64::MAX; n];
    let mut deque = std::collections::VecDeque::new();
    dist[from as usize] = 0;
    deque.push_back(from);
    while let Some(cur) = deque.pop_front() {
        if cur == to {
            return Some(dist[cur as usize]);
        }
        for &next in board.neighbors(cur) {
            let cost = match state.occupant(next) {
                Some(p) if p == player => 0,
                None => 1,
                _ => continue,
            };
            let nd = dist[cur as usize] + cost;
            if nd < dist[next as usize] {
                dist[next as usize] = nd;
                if cost == 0 {
                    deque.push_front(next);
                } else {
                    deque.push_back(next);
                }
            }
        }
    }
    None
}

impl Bot for TerritoryBot {
    fn choose(&mut self, game: &Game) -> Option<Move> {
        let me = game.to_move();
        let mut candidates: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m, Move::Place(_) | Move::Sever(_)))
            .collect();
        if candidates.is_empty() {
            return game
                .legal_moves()
                .contains(&Move::Pass)
                .then_some(Move::Pass);
        }
        candidates.shuffle(&mut self.rng); // tie-break variety

        // Pass 1: static eval of every candidate.
        let mut scored: Vec<(f64, Move)> = Vec::with_capacity(candidates.len());
        for &mv in &candidates {
            let mut sim = clone_game(game);
            if sim.play(mv).is_err() {
                continue;
            }
            scored.push((evaluate(&sim, me), mv));
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Pass 2: opponent's best static reply on the top candidates
        // (cheap 2-ply lookahead — punishes self-atari and hanging cuts).
        let width = self.reply_width.min(scored.len());
        let mut best: Option<(f64, Move)> = None;
        for &(_, mv) in scored.iter().take(width) {
            let mut sim = clone_game(game);
            if sim.play(mv).is_err() {
                continue;
            }
            // Opponent reply: sample their strongest from a static scan of
            // a bounded candidate set (their legal placements near action).
            let reply_eval = worst_case_after_reply(&sim, me);
            if best.is_none() || reply_eval > best.unwrap().0 {
                best = Some((reply_eval, mv));
            }
        }
        best.map(|(_, mv)| mv).or_else(|| {
            game.legal_moves()
                .contains(&Move::Pass)
                .then_some(Move::Pass)
        })
    }
}

/// Evaluate `sim` for `me` assuming the opponent makes their best static
/// reply among a bounded sample (all their legal placements adjacent to any
/// stone, plus severs, capped at 24 by shuffle-free deterministic order).
fn worst_case_after_reply(sim: &Game, me: Player) -> f64 {
    if sim.result().is_some() {
        return evaluate(sim, me);
    }
    let board = sim.board();
    let state = sim.state();
    // Candidate replies: nodes adjacent to any stone (the action frontier).
    let mut frontier: Vec<Move> = Vec::new();
    for n in 0..board.node_count() as NodeId {
        if state.occupant(n).is_some() {
            continue;
        }
        if board
            .neighbors(n)
            .iter()
            .any(|&nb| state.occupant(nb).is_some())
        {
            frontier.push(Move::Place(n));
        }
        if frontier.len() >= 24 {
            break;
        }
    }
    let mut worst = f64::INFINITY;
    let mut any = false;
    for mv in frontier {
        if sim.validate(&mv).is_err() {
            continue;
        }
        let mut sim2 = clone_game(sim);
        if sim2.play(mv).is_err() {
            continue;
        }
        let e = evaluate(&sim2, me);
        if e < worst {
            worst = e;
        }
        any = true;
    }
    if any {
        worst
    } else {
        evaluate(sim, me)
    }
}
