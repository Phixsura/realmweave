//! One-decision latency for the client's board size.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tooling
use realmweave_bot as bot;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move};
fn main() {
    let def = boardgen::generate_trinity(14).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let cfg =
        GameConfig::new(board.definition().id.clone()).with_ruleset(realmweave_core::TRINITY_Y_V4);
    let mut game = Game::new(board, cfg).unwrap();
    // 20 opening moves
    for i in 0..20u16 {
        let legal: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m, Move::Place(_)))
            .collect();
        let mv = legal[(i as usize * 7919) % legal.len()];
        game.play(mv).unwrap();
    }
    let t = std::time::Instant::now();
    let mv = bot::choose_move(&game, 42);
    println!("decision: {:?} in {:?}", mv, t.elapsed());
}
