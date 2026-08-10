//! Tutorial step conditions verified against a real scripted TRIFORCE game
//! through the same core APIs the tutorial panel reads (the tutorial now
//! runs on the merged-triangle flagship).
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

use realmweave_core::rules::Triforce;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, Player, TRIFORCE_V5};

#[test]
fn scripted_tutorial_conditions_hold() {
    let side = 10usize; // menu maps size 19 → triforce side 10
    let def = boardgen::generate_triforce(side).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let cfg = GameConfig::new(board.definition().id.clone()).with_ruleset(TRIFORCE_V5);
    let mut game = Game::new(board, cfg).unwrap();
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;

    // Dark filler: interior cells rows 1..4 (none touch big sides).
    let mut dark_cells = Vec::new();
    for r in 2..7 {
        for c in 1..r {
            dark_cells.push(idx(r, c));
        }
    }
    let mut di = 0;
    let mut dark = move || {
        di += 1;
        dark_cells[di - 1]
    };

    // FirstStone: human (Light) at even move indices.
    game.play(Move::Place(idx(6, 3))).unwrap(); // heart-ish interior
    assert_eq!(game.state().move_log.len() % 2, 1);
    game.play(Move::Place(dark())).unwrap();

    // TouchTwoSides: build to the left edge then along the bottom.
    for r in 7..side {
        game.play(Move::Place(idx(r, 0))).unwrap(); // toward bottom-left
        game.play(Move::Place(dark())).unwrap();
    }
    // Group now touches left (c==0) + bottom (r==9): two sides, len > 1.
    let (sides, len) = Triforce::weave_progress(game.board(), game.state(), Player::Light);
    assert!(sides >= 2, "left edge run touches two sides");
    assert!(len >= 3, "genuine group, not a lone corner");

    // Win: extend along the bottom row to the right side.
    for c in 1..side {
        game.play(Move::Place(idx(side - 1, c))).unwrap();
        if game.result().is_some() {
            break;
        }
        game.play(Move::Place(dark())).unwrap();
    }
    assert_eq!(
        Triforce::weaver(game.board(), game.state()),
        Some(Player::Light)
    );
    assert!(matches!(
        game.result(),
        Some(realmweave_core::GameResult::Win {
            player: Player::Light,
            ..
        })
    ));
}
