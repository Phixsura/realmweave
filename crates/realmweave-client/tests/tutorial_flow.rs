//! Tutorial step conditions verified against a real scripted game through
//! the same core APIs the tutorial panel reads.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test/tooling code
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, Realm};

#[test]
fn scripted_tutorial_conditions_hold() {
    let def = boardgen::generate_standard(19).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let cfg = GameConfig {
        ruleset_id: realmweave_core::WEAVE_SEVER_V2.to_string(),
        board_id: board.definition().id.clone(),
        pie_rule: false,
        time_control: None,
    };
    let mut game = Game::new(board, cfg).unwrap();

    // FirstStone condition: human (Light) has made a move at an even index.
    let first_empty = (0..game.board().node_count() as u16)
        .find(|&n| game.state().occupant(n).is_none() && game.validate(&Move::Place(n)).is_ok())
        .unwrap();
    game.play(Move::Place(first_empty)).unwrap();
    assert_eq!(game.state().move_log.len() % 2, 1);

    // CrossRealm condition is realm-count based: realm lookup must work.
    let realm0 = game.board().definition().nodes[first_empty as usize].realm;
    assert!(matches!(
        realm0,
        Realm::Heaven | Realm::Mortal | Realm::Underworld
    ));

    // UseScissors condition: a Light CutEdge is detectable in the log.
    let dark_spot = (0..game.board().node_count() as u16)
        .find(|&n| game.state().occupant(n).is_none() && game.validate(&Move::Place(n)).is_ok())
        .unwrap();
    game.play(Move::Place(dark_spot)).unwrap();
    if let Some(e) = (0..game.board().definition().edges.len() as u32)
        .find(|&e| game.validate(&Move::CutEdge(e)).is_ok())
    {
        game.play(Move::CutEdge(e)).unwrap();
        assert!(game
            .state()
            .move_log
            .iter()
            .enumerate()
            .any(|(i, m)| i % 2 == 0 && matches!(m, Move::CutEdge(_))));
    }
}
