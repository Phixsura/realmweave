//! Triforce v5: single-Y win, death rule, region metadata, replay.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use realmweave_core::rules::Triforce;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, Player, TRIFORCE_V5};

fn new_game(side: usize) -> Game {
    let b = BoardGraph::new(boardgen::generate_triforce(side).unwrap()).unwrap();
    let config = GameConfig::new(b.definition().id.clone()).with_ruleset(TRIFORCE_V5);
    Game::new(b, config).unwrap()
}

#[test]
fn boards_validate_and_regions_partition() {
    for side in [8usize, 14, 22] {
        let def = boardgen::generate_triforce(side).unwrap();
        realmweave_core::validate_board(&def).unwrap();
        let n = def.nodes.len();
        let mut counts = [0usize; 4];
        for id in 0..n as u16 {
            counts[boardgen::triforce_region(side, id)] += 1;
        }
        assert_eq!(counts.iter().sum::<usize>(), n);
        // three realms equal-sized; heart smaller
        assert_eq!(counts[0], counts[1]);
        assert_eq!(counts[1], counts[2]);
        assert!(counts[3] > 0 && counts[3] < counts[0] + 1);
    }
}

#[test]
fn bottom_row_y_wins_outright() {
    let side = 8;
    let mut game = new_game(side);
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    // Light claims the bottom row (touches left+right+bottom); Dark fills
    // distinct interior cells (rows 1-2 hold 5 cells: enough for 7 replies
    // plus... use rows 1..3 = 2+3+4 = 9 cells).
    let mut dark_cells = Vec::new();
    for r in 1..4 {
        for c in 0..=r {
            dark_cells.push(idx(r, c));
        }
    }
    for (di, c) in (0..side).enumerate() {
        game.play(Move::Place(idx(side - 1, c))).unwrap();
        if game.result().is_some() {
            break;
        }
        game.play(Move::Place(dark_cells[di])).unwrap();
    }
    assert_eq!(
        game.result(),
        Some(realmweave_core::GameResult::Win {
            player: Player::Light,
            reason: realmweave_core::WinReason::RealmWeave
        })
    );
}

#[test]
fn capture_works_on_merged_board() {
    let side = 8;
    let mut game = new_game(side);
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    let target = idx(3, 1);
    let ring = [
        idx(3, 0),
        idx(3, 2),
        idx(2, 0),
        idx(2, 1),
        idx(4, 1),
        idx(4, 2),
    ];
    game.play(Move::Place(ring[0])).unwrap(); // L
    game.play(Move::Place(target)).unwrap(); // D
    let mut filler = side - 1; // bottom row cells for Dark
    for r in ring.iter().skip(1) {
        game.play(Move::Place(*r)).unwrap(); // L
        game.play(Move::Place(idx(7, filler))).unwrap(); // D far
        filler -= 1;
    }
    assert_eq!(game.state().occupant(target), None, "surrounded stone dies");
    assert_eq!(game.state().captures[0], 1);
}

#[test]
fn weaver_scan_matches_engine_result() {
    let side = 8;
    let mut game = new_game(side);
    let mut seed = 7u64;
    while game.result().is_none() {
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
        game.play(legal[(seed % legal.len() as u64) as usize])
            .unwrap();
    }
    let result_winner = match game.result() {
        Some(realmweave_core::GameResult::Win { player, .. }) => Some(player),
        _ => None,
    };
    assert_eq!(Triforce::weaver(game.board(), game.state()), result_winner);
}

#[test]
fn replay_reproduces_state() {
    let mut game = new_game(8);
    let mut seed = 0xFEEDu64;
    for _ in 0..40 {
        if game.result().is_some() {
            break;
        }
        let legal: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m, Move::Place(_)))
            .collect();
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        game.play(legal[(seed % legal.len() as u64) as usize])
            .unwrap();
    }
    let b = BoardGraph::new(boardgen::generate_triforce(8).unwrap()).unwrap();
    let replayed = Game::replay(b, game.config().clone(), &game.state().move_log).unwrap();
    assert_eq!(replayed.state(), game.state());
}
