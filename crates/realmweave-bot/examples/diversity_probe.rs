//! Does MCTS v5 play DIVERSE games, or funnel into one groove?
//! Measure: pairwise first-divergence ply + move-set Jaccard across seeds.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use realmweave_bot as bot;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move};

fn main() {
    let budget = bot::mcts::MctsConfig {
        playouts: 500,
        c: 0.9,
    };
    let mut logs: Vec<Vec<Move>> = Vec::new();
    for g in 0..5u64 {
        let def = boardgen::generate_triforce(16).unwrap();
        let board = BoardGraph::new(def).unwrap();
        let cfg = GameConfig::new(board.definition().id.clone())
            .with_ruleset(realmweave_core::TRIFORCE_V5);
        let mut gm = Game::new(board, cfg).unwrap();
        while gm.result().is_none() && gm.state().ply < 500 {
            let seed = g.wrapping_mul(0x9E3779B97F4A7C15) ^ gm.state().ply as u64;
            let Some(mv) = bot::choose_move_with_budget(&gm, seed, budget) else {
                break;
            };
            let _ = gm.play(mv);
        }
        println!(
            "g{g}: {} moves, first 6: {:?}",
            gm.state().move_log.len(),
            &gm.state().move_log[..6.min(gm.state().move_log.len())]
        );
        logs.push(gm.state().move_log.clone());
    }
    for i in 0..logs.len() {
        for j in (i + 1)..logs.len() {
            let n = logs[i].len().min(logs[j].len());
            let first_diff = (0..n).find(|&k| logs[i][k] != logs[j][k]).unwrap_or(n);
            let a: std::collections::HashSet<_> = logs[i].iter().collect();
            let b: std::collections::HashSet<_> = logs[j].iter().collect();
            let jac = a.intersection(&b).count() as f64 / a.union(&b).count() as f64;
            println!(
                "{i}~{j}: diverge at ply {first_diff}, move-set overlap {:.0}%",
                jac * 100.0
            );
        }
    }
}
