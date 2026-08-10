//! Depth probe: does the engine UNDERSTAND anything, or only win vs random?
//! T1: budget scaling — 8000 must beat 800 convincingly (else the tree adds nothing).
//! T2: life&death — a group in atari-in-2: does the side to move kill/save it?
#![allow(clippy::unwrap_used, clippy::expect_used)]
use realmweave_bot as bot;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, Player};

fn game(side: usize) -> Game {
    let def = boardgen::generate_triforce(side).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let cfg =
        GameConfig::new(board.definition().id.clone()).with_ruleset(realmweave_core::TRIFORCE_V5);
    Game::new(board, cfg).unwrap()
}

fn main() {
    // T1: 8000 vs 800, both colors, 6 games
    let hi = bot::mcts::MctsConfig {
        playouts: 6000,
        c: 0.9,
    };
    let lo = bot::mcts::MctsConfig {
        playouts: 600,
        c: 0.9,
    };
    let mut hi_wins = 0;
    let games = 6;
    for g in 0..games {
        let hi_is_light = g % 2 == 0;
        let mut gm = game(16);
        while gm.result().is_none() && gm.state().ply < 600 {
            let seed = 0xA11u64 ^ (g as u64) << 32 ^ gm.state().ply as u64;
            let budget = if (gm.to_move() == Player::Light) == hi_is_light {
                hi
            } else {
                lo
            };
            let Some(mv) = bot::choose_move_with_budget(&gm, seed, budget) else {
                break;
            };
            let _ = gm.play(mv);
        }
        let hi_won = matches!(gm.result(),
            Some(realmweave_core::GameResult::Win { player, .. }) if (player == Player::Light) == hi_is_light);
        if hi_won {
            hi_wins += 1;
        }
        println!(
            "T1 g{g}: hi_as={} won={}",
            if hi_is_light { "L" } else { "D" },
            hi_won
        );
    }
    println!("== T1: 6000-playout beats 600-playout {hi_wins}/{games} ==");

    // T2: life & death. Craft: Dark group with exactly 2 liberties, Light to move.
    // Build on side 10: dark chain at (4,1),(4,2); light surrounds except 2 libs.
    let mut gm = game(10);
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    // Script: alternate to build the shape legally.
    // Dark stones: (4,1),(4,2). Light: (3,0),(3,1),(3,2),(4,0),(5,2),(5,3) → libs of dark chain: (4,3)? no wait (4,3) exists (row4 c3<=4) and (5,1).
    let seq: Vec<(u16, bool)> = vec![
        (idx(3, 0), true),
        (idx(4, 1), false),
        (idx(3, 1), true),
        (idx(4, 2), false),
        (idx(3, 2), true),
        (idx(8, 4), false),
        (idx(4, 0), true),
        (idx(8, 5), false),
        (idx(5, 2), true),
        (idx(8, 6), false),
        (idx(5, 3), true),
        (idx(8, 7), false),
        (idx(3, 3), true),
        (idx(8, 8), false), // close lib (4,3)'s upper support? (4,3) empty still
        (idx(5, 4), true),
        (idx(8, 3), false),
    ];
    for (mv, is_light) in seq {
        assert_eq!(gm.to_move() == Player::Light, is_light);
        gm.play(Move::Place(mv)).unwrap();
    }
    // Dark chain (4,1),(4,2) liberties now: (4,3)? row4 c3: neighbors of (4,2) = (4,1),(4,3),(3,1),(3,2),(5,2),(5,3) → (4,3) empty ✓; (5,1): neighbors of (4,1)=(4,0)L,(4,2)D,(3,0)L,(3,1)L,(5,1),(5,2)L → (5,1) empty ✓. 2 libs: (4,3),(5,1).
    println!(
        "T2 position built. Dark chain libs should be 2: (4,3)={} (5,1)={}",
        gm.state().occupant(idx(4, 3)).is_none(),
        gm.state().occupant(idx(5, 1)).is_none()
    );
    // Light to move: killing requires playing one lib then the other. Does MCTS start the kill?
    let mv = bot::choose_move_with_budget(
        &gm,
        99,
        bot::mcts::MctsConfig {
            playouts: 4000,
            c: 0.9,
        },
    )
    .unwrap();
    println!(
        "T2 Light's choice: {:?} (kill-start = Place({}) or Place({}))",
        mv,
        idx(4, 3),
        idx(5, 1)
    );
    // And if Dark to move instead (pass-simulate by flipping: play a Light tenuki far away first)
    gm.play(Move::Place(idx(9, 0))).unwrap(); // Light tenuki
    let mv2 = bot::choose_move_with_budget(
        &gm,
        77,
        bot::mcts::MctsConfig {
            playouts: 4000,
            c: 0.9,
        },
    )
    .unwrap();
    println!(
        "T2 Dark's defense: {:?} (escape via ({},{}) area or counter)",
        mv2, 4, 3
    );
}
