//! Tutorial step conditions verified against a real scripted trinity game
//! through the same core APIs the tutorial panel reads.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

use realmweave_core::rules::TrinityY;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, Player, TRINITY_Y_V4};

#[test]
fn scripted_tutorial_conditions_hold() {
    let side = 8usize;
    let def = boardgen::generate_trinity(side).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let cfg = GameConfig::new(board.definition().id.clone()).with_ruleset(TRINITY_Y_V4);
    let mut game = Game::new(board, cfg).unwrap();
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;

    let per = side * (side + 1) / 2;
    // Light builds down the left edge; Dark fills distinct realm-2 cells.
    let mut dark_i = 0usize;
    let mut dark = move || {
        dark_i += 1;
        (2 * per + dark_i - 1) as u16
    };

    // FirstStone: human (Light) moves at even indices.
    game.play(Move::Place(idx(3, 1))).unwrap();
    assert_eq!(game.state().move_log.len() % 2, 1);
    game.play(Move::Place(dark())).unwrap();

    // TouchTwoSides: reach the left edge, then the bottom row.
    game.play(Move::Place(idx(3, 0))).unwrap(); // touches left
    game.play(Move::Place(dark())).unwrap();
    for r in 4..side {
        game.play(Move::Place(idx(r, 0))).unwrap();
        game.play(Move::Place(dark())).unwrap();
    }
    // bottom-left corner touches left + bottom = 2 sides
    let side_mask = boardgen::trinity_sides(side, idx(side - 1, 0));
    assert!(side_mask.count_ones() >= 2, "corner touches two sides");

    // WinRealm condition: no Y yet; complete the bottom row → Y.
    assert!(TrinityY::realm_winner(game.board(), game.state(), 0).is_none());
    for c in 1..side {
        game.play(Move::Place(idx(side - 1, c))).unwrap();
        if c < side - 1 {
            game.play(Move::Place(dark())).unwrap();
        }
    }
    assert_eq!(
        TrinityY::realm_winner(game.board(), game.state(), 0),
        Some(Player::Light),
        "left edge + bottom row = Y"
    );
}
