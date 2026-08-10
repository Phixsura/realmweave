//! Triforce self-play: length, captures, decisiveness, realm-crossing.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use realmweave_bot as bot;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move};

fn main() {
    let games: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let side: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(22);
    let budget = bot::mcts::MctsConfig {
        playouts: 800,
        c: 0.9,
    };
    for g in 0..games {
        let def = boardgen::generate_triforce(side).unwrap();
        let board = BoardGraph::new(def).unwrap();
        let cfg = GameConfig::new(board.definition().id.clone())
            .with_ruleset(realmweave_core::TRIFORCE_V5);
        let mut game = Game::new(board, cfg).unwrap();
        while game.result().is_none() && game.state().ply < 800 {
            let seed = 0xD0E1u64 ^ (g as u64) << 40 ^ game.state().ply as u64;
            let Some(mv) = bot::choose_move_with_budget(&game, seed, budget) else {
                break;
            };
            if game.play(mv).is_err() {
                let _ = game.play(Move::Pass);
            }
        }
        println!(
            "g{g}: {:?} moves={} captures={:?}",
            game.result(),
            game.state().move_log.len(),
            game.state().captures
        );
    }
}
