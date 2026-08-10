#![allow(clippy::needless_range_loop)]
//! Trinity Y (v4): the Y theorem in play — decisiveness, realm scoring,
//! match win, replay determinism.

use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, Player, TRINITY_Y_V4};

fn new_game(side: usize) -> Game {
    let b = BoardGraph::new(boardgen::generate_trinity(side).unwrap()).unwrap();
    let config = GameConfig::new(b.definition().id.clone()).with_ruleset(TRINITY_Y_V4);
    Game::new(b, config).unwrap()
}

/// Full random fill via legal play must ALWAYS produce a winner (the Y
/// theorem: no realm can stay undecided on a full triangle, so someone
/// reaches 2 realms before or at board-full).
#[test]
fn random_games_always_decisive() {
    for seed0 in 0..10u64 {
        let mut game = new_game(6);
        let mut seed = seed0.wrapping_mul(0x9E3779B97F4A7C15) | 1;
        while game.result().is_none() {
            let moves: Vec<Move> = game
                .legal_moves()
                .into_iter()
                .filter(|m| matches!(m, Move::Place(_)))
                .collect();
            assert!(
                !moves.is_empty(),
                "board filled without a match winner — Y theorem violated"
            );
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mv = moves[(seed % moves.len() as u64) as usize];
            game.play(mv).unwrap();
        }
        let result = game.result().unwrap();
        assert!(matches!(result, realmweave_core::GameResult::Win { .. }));
    }
}

/// Winning one realm scores it; two realms end the match.
#[test]
fn realm_scores_track_and_two_realms_win() {
    let side = 5;
    let mut game = new_game(side);
    // Light fills realm 0's left side then bottom-right diagonal: build an
    // explicit Y in realm 0 while Dark plays in realm 2 (far corner cells).
    // Simplest Y on side 5: entire bottom row + a spine to the apex.
    let per = side * (side + 1) / 2;
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    // Light: bottom row (touches left+right+bottom... bottom row touches
    // left at c=0 and right at c=r): one row IS a Y on a triangle!
    let bottom: Vec<u16> = (0..side).map(|c| idx(side - 1, c)).collect();
    // Dark: bottom row of realm 2 (same shape, offset 2*per).
    let dark_bottom: Vec<u16> = (0..side)
        .map(|c| idx(side - 1, c) + 2 * per as u16)
        .collect();
    for i in 0..side {
        game.play(Move::Place(bottom[i])).unwrap();
        if game.result().is_some() {
            break;
        }
        game.play(Move::Place(dark_bottom[i])).unwrap();
    }
    // Light finished realm 0's bottom row first → scores [1, ...]; then
    // Dark completes realm 2 → [1, 1]. Nobody has 2: game continues.
    assert!(game.result().is_none());
    assert_eq!(game.state().layers, [1, 1], "one realm each");
    // Light now takes realm 1's bottom row while Dark plays row 1 (apex
    // side, no contact with the bottom row = no capture interaction).
    let mid_bottom: Vec<u16> = (0..side).map(|c| idx(side - 1, c) + per as u16).collect();
    let far: Vec<u16> = (0..2).map(|c| idx(1, c) + per as u16).collect();
    for i in 0..side {
        game.play(Move::Place(mid_bottom[i])).unwrap();
        if game.result().is_some() {
            break;
        }
        let _ = game.play(Move::Place(far[i % far.len()]));
        if game.result().is_some() {
            break;
        }
        if i >= far.len() {
            // Dark passes once out of scripted moves
            let _ = game.play(Move::Pass);
        }
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
fn replay_reproduces_state() {
    let mut game = new_game(6);
    let mut seed = 0xFACEu64;
    for _ in 0..30 {
        if game.result().is_some() {
            break;
        }
        let moves: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m, Move::Place(_)))
            .collect();
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        game.play(moves[(seed % moves.len() as u64) as usize])
            .unwrap();
    }
    let log = game.state().move_log.clone();
    let b = BoardGraph::new(boardgen::generate_trinity(6).unwrap()).unwrap();
    let replayed = Game::replay(b, game.config().clone(), &log).unwrap();
    assert_eq!(replayed.state(), game.state());
}

/// Death: surrounding a lone stone captures it.
#[test]
fn surrounded_stone_dies() {
    let side = 6;
    let mut game = new_game(side);
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    // Dark stone at interior (2,1); Light surrounds with its 6 neighbors:
    // (2,0),(2,2),(1,0),(1,1),(3,1),(3,2)
    let target = idx(2, 1);
    let ring = [
        idx(2, 0),
        idx(2, 2),
        idx(1, 0),
        idx(1, 1),
        idx(3, 1),
        idx(3, 2),
    ];
    // Light opens far away; Dark plays the target; Light rings it while
    // Dark plays elsewhere (realm 2).
    let far = |k: usize| (2 * (side * (side + 1) / 2) + k) as u16;
    game.play(Move::Place(ring[0])).unwrap(); // L
    game.play(Move::Place(target)).unwrap(); // D
    let mut fk = 0;
    for i in 1..6 {
        game.play(Move::Place(ring[i])).unwrap(); // L
        assert!(game.result().is_none());
        if i < 5 {
            game.play(Move::Place(far(fk))).unwrap(); // D elsewhere
            fk += 1;
        }
    }
    // 6th ring stone just captured the Dark stone.
    assert_eq!(game.state().occupant(target), None, "captured");
    assert_eq!(game.state().captures, [1, 0]);
    // The point is immediately playable by Light later but by Dark it
    // would be suicide (ring intact):
    assert!(
        matches!(
            game.validate(&Move::Place(target)),
            Err(realmweave_core::rules::RuleError::SuicideMove(_))
        ),
        "refilling into the ring is suicide for Dark"
    );
}

/// Sealed realm: once a realm is won, its stones are immortal and the
/// realm is closed.
#[test]
fn sealed_realm_is_closed_and_immortal() {
    let side = 5;
    let mut game = new_game(side);
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    let bottom: Vec<u16> = (0..side).map(|c| idx(side - 1, c)).collect();
    let per = side * (side + 1) / 2;
    // Light wins realm 0 with the bottom row; Dark plays realm 1.
    for i in 0..side {
        game.play(Move::Place(bottom[i])).unwrap();
        if i < side - 1 {
            game.play(Move::Place(idx(side - 1, i) + per as u16))
                .unwrap();
        }
    }
    assert_eq!(game.state().layers[0], 1, "realm 0 sealed by Light");
    // Any placement in realm 0 is now illegal (closed).
    let interior = idx(1, 0);
    assert!(game.validate(&Move::Place(interior)).is_err());
}
