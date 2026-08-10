//! Engine hot-path benchmarks: the numbers that gate ruleset and board
//! growth. Run: cargo bench -p realmweave-core

#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)] // bench harness
use criterion::{criterion_group, criterion_main, Criterion};
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move};
use std::hint::black_box;

fn mid_game(ruleset: &str, moves: usize) -> Game {
    let def = if ruleset == realmweave_core::TRINITY_Y_V4 {
        boardgen::generate_trinity(14).expect("trinity board")
    } else {
        boardgen::generate_standard(91).expect("standard board")
    };
    let board = BoardGraph::new(def).expect("valid board");
    let cfg = GameConfig::new(board.definition().id.clone()).with_ruleset(ruleset);
    let mut game = Game::new(board, cfg).expect("game");
    let mut seed = 0x5EEDu64;
    for _ in 0..moves {
        if game.result().is_some() {
            break;
        }
        let legal: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m, Move::Place(_)))
            .collect();
        if legal.is_empty() {
            break;
        }
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mv = legal[(seed % legal.len() as u64) as usize];
        let _ = game.play(mv);
    }
    game
}

fn bench_engine(c: &mut Criterion) {
    for ruleset in [
        realmweave_core::TRINITY_Y_V4,
        realmweave_core::WEAVE_LAYERS_V3,
    ] {
        let game = mid_game(ruleset, 60);
        c.bench_function(&format!("{ruleset}/legal_moves"), |b| {
            b.iter(|| black_box(game.legal_moves()))
        });
        let mv = game
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m, Move::Place(_)))
            .expect("placement available");
        c.bench_function(&format!("{ruleset}/validate"), |b| {
            b.iter(|| black_box(game.validate(&mv)))
        });
        c.bench_function(&format!("{ruleset}/replay_60"), |b| {
            let log = game.state().move_log.clone();
            let cfg = game.config().clone();
            b.iter(|| {
                let def = if ruleset == realmweave_core::TRINITY_Y_V4 {
                    boardgen::generate_trinity(14).expect("trinity board")
                } else {
                    boardgen::generate_standard(91).expect("standard board")
                };
                let bd = BoardGraph::new(def).expect("valid board");
                black_box(Game::replay(bd, cfg.clone(), &log).expect("replays"))
            })
        });
    }
}

criterion_group!(benches, bench_engine);
criterion_main!(benches);
