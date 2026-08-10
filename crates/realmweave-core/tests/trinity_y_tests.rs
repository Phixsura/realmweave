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
    let dark_bottom: Vec<u16> = (0..side).map(|c| idx(side - 1, c) + 2 * per as u16).collect();
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
    // Light now takes realm 1's bottom row while Dark fills realm 1 interior.
    let mid_bottom: Vec<u16> = (0..side).map(|c| idx(side - 1, c) + per as u16).collect();
    let interior: Vec<u16> = (0..side - 1).map(|c| idx(side - 2, c) + per as u16).collect();
    for i in 0..side {
        game.play(Move::Place(mid_bottom[i])).unwrap();
        if game.result().is_some() {
            break;
        }
        let _ = game.play(Move::Place(interior[i.min(interior.len() - 1)]));
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
        game.play(moves[(seed % moves.len() as u64) as usize]).unwrap();
    }
    let log = game.state().move_log.clone();
    let b = BoardGraph::new(boardgen::generate_trinity(6).unwrap()).unwrap();
    let replayed = Game::replay(b, game.config().clone(), &log).unwrap();
    assert_eq!(replayed.state(), game.state());
}
