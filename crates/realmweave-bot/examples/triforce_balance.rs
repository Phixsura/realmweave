//! v5 balance measurement: first-player winrate at MCTS level, and
//! heart-opening vs corner-opening strength (center dominance probe).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use realmweave_bot as bot;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move};

fn play(
    first_move: Option<u16>,
    seed_base: u64,
    budget: bot::mcts::MctsConfig,
) -> (realmweave_core::GameResult, usize) {
    let def = boardgen::generate_triforce(22).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let cfg =
        GameConfig::new(board.definition().id.clone()).with_ruleset(realmweave_core::TRIFORCE_V5);
    let mut game = Game::new(board, cfg).unwrap();
    if let Some(mv) = first_move {
        game.play(Move::Place(mv)).unwrap();
    }
    while game.result().is_none() && game.state().ply < 800 {
        let seed = seed_base ^ game.state().ply as u64;
        let Some(mv) = bot::choose_move_with_budget(&game, seed, budget) else {
            break;
        };
        if game.play(mv).is_err() {
            let _ = game.play(Move::Pass);
        }
    }
    (
        game.result().unwrap_or(realmweave_core::GameResult::Draw),
        game.state().move_log.len(),
    )
}

fn main() {
    let budget = bot::mcts::MctsConfig {
        playouts: 500,
        c: 0.9,
    };
    let games: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);
    // A) free openings: first-player winrate
    let mut light = 0u32;
    for g in 0..games {
        let (r, n) = play(None, 0x1000 + g * 0x9E3779B9, budget);
        if matches!(
            r,
            realmweave_core::GameResult::Win {
                player: realmweave_core::Player::Light,
                ..
            }
        ) {
            light += 1;
        }
        println!("free g{g}: {r:?} ({n} moves)");
    }
    println!("== first player wins {light}/{games} ==");
    // B) heart vs corner-area opening strength (fixed first move, then winrate for Light)
    let side = 22usize;
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    let heart = idx(side * 3 / 4, side * 3 / 8); // deep heart-ish
    let corner = idx(2, 1); // heaven corner interior
    let edge = idx(side - 1, side / 2); // bottom edge middle
    for (name, mv) in [("heart", heart), ("corner", corner), ("edge", edge)] {
        let mut wins = 0u32;
        for g in 0..games {
            let (r, _) = play(Some(mv), 0x7000 + g * 0x51ED, budget);
            if matches!(
                r,
                realmweave_core::GameResult::Win {
                    player: realmweave_core::Player::Light,
                    ..
                }
            ) {
                wins += 1;
            }
        }
        println!("opening {name} (node {mv}): Light wins {wins}/{games}");
    }
}
