//! Monte-Carlo Tree Search bot (UCT) for balance experiments.
//!
//! Deliberately simple: full game state clones per node, uniform random
//! rollouts with a light connectivity bias. 183-node boards are small enough
//! that this is fast at the playout counts we need for balance statistics.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::Rng;
use realmweave_core::{BoardGraph, Game, GameConfig, GameResult, Move, Player};

use crate::bots::Bot;

pub struct MctsBot {
    pub rng: StdRng,
    /// Playouts per move decision.
    pub playouts: u32,
    /// UCT exploration constant.
    pub c: f64,
}

struct Node {
    mv: Option<Move>,
    parent: Option<usize>,
    children: Vec<usize>,
    untried: Vec<Move>,
    visits: u32,
    /// Wins from the perspective of the player who made `mv`.
    wins: f64,
    to_move: Player,
}

impl Bot for MctsBot {
    fn choose(&mut self, game: &Game) -> Option<Move> {
        let moves: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| !matches!(m, Move::Resign))
            .collect();
        match moves.len() {
            0 => return None,
            1 => return Some(moves[0]),
            _ => {}
        }

        let root_state = clone_game(game);
        let mut nodes = vec![Node {
            mv: None,
            parent: None,
            children: Vec::new(),
            untried: moves,
            visits: 0,
            wins: 0.0,
            to_move: game.to_move(),
        }];

        for _ in 0..self.playouts {
            let mut sim = clone_game(&root_state);
            // --- selection ---
            let mut current = 0usize;
            while nodes[current].untried.is_empty() && !nodes[current].children.is_empty() {
                current = self.select_uct(&nodes, current);
                if let Some(mv) = nodes[current].mv {
                    let _ = sim.play(mv);
                }
            }
            // --- expansion ---
            if !nodes[current].untried.is_empty() && sim.result().is_none() {
                let idx = self.rng.gen_range(0..nodes[current].untried.len());
                let mv = nodes[current].untried.swap_remove(idx);
                let _ = sim.play(mv);
                let child = Node {
                    mv: Some(mv),
                    parent: Some(current),
                    children: Vec::new(),
                    untried: sim
                        .legal_moves()
                        .into_iter()
                        .filter(|m| !matches!(m, Move::Resign))
                        .collect(),
                    visits: 0,
                    wins: 0.0,
                    to_move: sim.to_move(),
                };
                nodes.push(child);
                let child_idx = nodes.len() - 1;
                nodes[current].children.push(child_idx);
                current = child_idx;
            }
            // --- rollout ---
            let winner = self.rollout(&mut sim);
            // --- backpropagation ---
            let mut node = Some(current);
            while let Some(i) = node {
                nodes[i].visits += 1;
                if let (Some(parent), Some(_)) = (nodes[i].parent, nodes[i].mv) {
                    // The mover at this edge is the parent's to_move.
                    let mover = nodes[parent].to_move;
                    nodes[i].wins += match winner {
                        Some(w) if w == mover => 1.0,
                        None => 0.5,
                        _ => 0.0,
                    };
                }
                node = nodes[i].parent;
            }
        }

        // Most-visited child of root.
        nodes[0]
            .children
            .iter()
            .max_by_key(|&&c| nodes[c].visits)
            .and_then(|&c| nodes[c].mv)
    }
}

impl MctsBot {
    fn select_uct(&mut self, nodes: &[Node], parent: usize) -> usize {
        let ln_n = (nodes[parent].visits.max(1) as f64).ln();
        *nodes[parent]
            .children
            .iter()
            .max_by(|&&a, &&b| {
                let ua = uct(&nodes[a], ln_n, self.c);
                let ub = uct(&nodes[b], ln_n, self.c);
                ua.partial_cmp(&ub).unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("children non-empty")
    }

    /// Random playout to terminal state; returns winner (None = draw).
    fn rollout(&mut self, sim: &mut Game) -> Option<Player> {
        loop {
            if let Some(result) = sim.result() {
                return match result {
                    GameResult::Win { player, .. } => Some(player),
                    GameResult::Draw => None,
                };
            }
            let placements: Vec<Move> = sim
                .legal_moves()
                .into_iter()
                .filter(|m| matches!(m, Move::Place(_) | Move::Sever(_)))
                .collect();
            let &mv = placements.choose(&mut self.rng)?;
            let _ = sim.play(mv);
        }
    }
}

fn uct(node: &Node, ln_n: f64, c: f64) -> f64 {
    if node.visits == 0 {
        return f64::INFINITY;
    }
    let v = node.visits as f64;
    node.wins / v + c * (ln_n / v).sqrt()
}

/// Rebuild an owned copy of a game (Game is not Clone: it holds a boxed
/// ruleset). Deterministic replay makes this exact.
fn clone_game(game: &Game) -> Game {
    let board = BoardGraph::new(game.board().definition().clone()).expect("board");
    let config: GameConfig = game.config().clone();
    Game::replay(board, config, &game.state().move_log).expect("replay clone")
}
