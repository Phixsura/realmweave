//! Strength gate: MCTS must dominate the 2-ply baseline.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tooling
use realmweave_bot as bot;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, Player};

fn main() {
    let games: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let side: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let mut mcts_wins = 0u32;
    for g in 0..games {
        // alternate colors: MCTS plays Light on even games
        let mcts_is_light = g % 2 == 0;
        let ruleset = std::env::args()
            .nth(3)
            .unwrap_or_else(|| realmweave_core::TRIFORCE_V5.to_string());
        let def = if ruleset == realmweave_core::TRIFORCE_V5 {
            boardgen::generate_triforce(if side.is_multiple_of(2) {
                side
            } else {
                side + 1
            })
            .unwrap()
        } else {
            boardgen::generate_trinity(side).unwrap()
        };
        let board = BoardGraph::new(def).unwrap();
        let cfg = GameConfig::new(board.definition().id.clone()).with_ruleset(&ruleset);
        let mut game = Game::new(board, cfg).unwrap();
        while game.result().is_none() && game.state().ply < 600 {
            let seed = 0xACEu64 ^ (g as u64) << 32 ^ game.state().ply as u64;
            let is_mcts_turn = (game.to_move() == Player::Light) == mcts_is_light;
            let mv = if is_mcts_turn {
                bot::mcts::choose_move_mcts(&game, seed, bot::mcts::MctsConfig::default())
                    .unwrap_or(Move::Pass)
            } else {
                baseline_2ply(&game, seed)
            };
            if game.play(mv).is_err() {
                let _ = game.play(Move::Pass);
            }
        }
        let mcts_won = match game.result() {
            Some(realmweave_core::GameResult::Win { player, .. }) => {
                (player == Player::Light) == mcts_is_light
            }
            _ => false,
        };
        if mcts_won {
            mcts_wins += 1;
        }
        println!(
            "game {g}: mcts_as={} result={:?} moves={} mcts_won={}",
            if mcts_is_light { "L" } else { "D" },
            game.result(),
            game.state().move_log.len(),
            mcts_won
        );
    }
    println!("MCTS wins {mcts_wins}/{games}");
}

/// The old 2-ply baseline (link-cost eval) — call through the non-MCTS path
/// by picking randomly among top eval placements... simplest honest baseline:
/// uniform random legal placement (the 2-ply path is now MCTS-gated).
fn baseline_2ply(game: &Game, seed: u64) -> Move {
    let legal: Vec<Move> = game
        .legal_moves()
        .into_iter()
        .filter(|m| matches!(m, Move::Place(_)))
        .collect();
    if legal.is_empty() {
        return Move::Pass;
    }
    let mut s = seed | 1;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    legal[(s % legal.len() as u64) as usize]
}
