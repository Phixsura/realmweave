//! Deterministic (seeded) bots for self-play. The engine itself contains no
//! randomness; seeds live entirely in the simulation harness.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::Rng;
use realmweave_core::{Game, Move, NodeId, Player};

pub trait Bot {
    /// Whether this bot makes its own pie-rule swap decision (the batch
    /// runner must then NOT pre-empt it with the rollout estimator).
    fn handles_pie(&self) -> bool {
        false
    }

    fn choose(&mut self, game: &Game) -> Option<Move>;
}

fn placements(game: &Game) -> Vec<NodeId> {
    game.legal_moves()
        .into_iter()
        .filter_map(|m| match m {
            Move::Place(n) => Some(n),
            _ => None,
        })
        .collect()
}

fn severs(game: &Game) -> Vec<NodeId> {
    game.legal_moves()
        .into_iter()
        .filter_map(|m| match m {
            Move::Sever(n) => Some(n),
            _ => None,
        })
        .collect()
}

/// Uniform random placement.
pub struct RandomBot {
    pub rng: StdRng,
}

impl Bot for RandomBot {
    fn choose(&mut self, game: &Game) -> Option<Move> {
        placements(game)
            .choose(&mut self.rng)
            .map(|&n| Move::Place(n))
    }
}

/// Greedy connectivity bot: prefers the placement that minimizes the sum of
/// shortest-path distances (through own/empty nodes only) between its origin
/// components; falls back to blocking the opponent's best move.
pub struct GreedyBot {
    pub rng: StdRng,
    /// Chance to play a random move instead (exploration noise).
    pub epsilon: f64,
}

impl Bot for GreedyBot {
    fn choose(&mut self, game: &Game) -> Option<Move> {
        // Y-family boards have no origins, so the origin-pair connection
        // score is identically zero and "greedy" silently degrades to
        // uniform random — misleading for balance sweeps. Route those to
        // the real engine at a light budget instead.
        if game.board().definition().origins.is_empty() {
            return realmweave_bot::choose_move_with_budget(
                game,
                self.rng.gen(),
                realmweave_bot::mcts::MctsConfig {
                    playouts: 300,
                    c: 0.9,
                },
            );
        }
        let candidates = placements(game);
        if candidates.is_empty() {
            return game
                .legal_moves()
                .contains(&Move::Pass)
                .then_some(Move::Pass);
        }
        if self.rng.gen_bool(self.epsilon) {
            return candidates.choose(&mut self.rng).map(|&n| Move::Place(n));
        }
        let me = game.to_move();
        let mut best: Vec<Move> = Vec::new();
        let mut best_score = i64::MAX;
        for &node in &candidates {
            let my_gain = connection_score(game, me, Some(node));
            let their_gain = connection_score(game, me.opponent(), Some(node));
            // Lower own connection distance is good; raising the opponent's
            // is half-weighted.
            let score = my_gain * 2 - their_gain;
            match score.cmp(&best_score) {
                std::cmp::Ordering::Less => {
                    best_score = score;
                    best = vec![Move::Place(node)];
                }
                std::cmp::Ordering::Equal => best.push(Move::Place(node)),
                std::cmp::Ordering::Greater => {}
            }
        }
        // Sever option (sever ruleset): removing an enemy stone that most
        // hurts their connectivity, evaluated with the same score.
        for target in severs(game) {
            if let Ok(hypothetical) = hypothetical_without(game, target) {
                let my_gain = connection_score(&hypothetical, me, None);
                let their_gain = connection_score(&hypothetical, me.opponent(), None);
                let score = my_gain * 2 - their_gain;
                match score.cmp(&best_score) {
                    std::cmp::Ordering::Less => {
                        best_score = score;
                        best = vec![Move::Sever(target)];
                    }
                    std::cmp::Ordering::Equal => best.push(Move::Sever(target)),
                    std::cmp::Ordering::Greater => {}
                }
            }
        }
        best.choose(&mut self.rng).copied()
    }
}

/// Clone the game with one enemy stone removed (for sever evaluation).
fn hypothetical_without(game: &Game, target: NodeId) -> Result<Game, ()> {
    use realmweave_core::BoardGraph;
    let board = BoardGraph::new(game.board().definition().clone()).map_err(|_| ())?;
    let mut sim =
        Game::replay(board, game.config().clone(), &game.state().move_log).map_err(|_| ())?;
    sim.play(Move::Sever(target)).map_err(|_| ())?;
    Ok(sim)
}

/// Sum over origin pairs of the shortest path length routed through nodes
/// that are the player's own or empty (i.e. buildable). `extra` pretends one
/// more stone belongs to the player. Unreachable pairs cost a large penalty.
fn connection_score(game: &Game, player: Player, extra: Option<NodeId>) -> i64 {
    let board = game.board();
    let state = game.state();
    let origins = board.definition().origins_of(player);
    let passable = |n: NodeId| -> bool {
        if Some(n) == extra {
            return true;
        }
        // Petrified nodes read as empty in `occupant`, but only opponent
        // fossils are roads for us — our own are walls, and unpetrified
        // rules never set the flag, so this is a no-op for classic.
        if state.is_petrified(n) {
            return state.fossil_road_for(n, player);
        }
        match state.occupant(n) {
            None => true,
            Some(p) => p == player,
        }
    };
    // Own stones cost 0, empty nodes cost 1 (we would have to spend a move).
    // Dijkstra-lite with 0/1 weights (deque BFS).
    let mut total: i64 = 0;
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            let dist = zero_one_bfs(game, origins[i], origins[j], &passable, player, extra);
            total += dist.unwrap_or(1_000);
        }
    }
    total
}

fn zero_one_bfs(
    game: &Game,
    from: NodeId,
    to: NodeId,
    passable: &dyn Fn(NodeId) -> bool,
    player: Player,
    extra: Option<NodeId>,
) -> Option<i64> {
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
        // Cut edges are PERMANENT in sever rulesets: scoring through them
        // means reinforcing routes that no longer exist on the board.
        for next in board.live_neighbors(cur, &state.cut_edges) {
            if !passable(next) {
                continue;
            }
            let mine = state.occupant(next) == Some(player) || Some(next) == extra;
            let cost = if mine { 0 } else { 1 };
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
