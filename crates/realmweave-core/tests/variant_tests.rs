//! Tests for the doubleweave, sever, and territory rule variants.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test/tooling code
use realmweave_core::board::BoardGraph;
use realmweave_core::boardgen;
use realmweave_core::rules::RuleError;
use realmweave_core::{Game, GameConfig, Move, NodeId, Player, Realm, SEVER_V1};

fn board(size: usize) -> BoardGraph {
    BoardGraph::new(boardgen::generate_standard(size).unwrap()).unwrap()
}

fn new_game(size: usize, ruleset: &str) -> Game {
    let b = board(size);
    let config = GameConfig::new(b.definition().id.clone()).with_ruleset(ruleset);
    Game::new(b, config).unwrap()
}

fn node(game: &Game, realm: Realm, ax: [i32; 2]) -> NodeId {
    game.board().axial_index()[&(realm, ax)]
}

/// Single-route weave path for Light on the 19 board (as in game_tests).
fn light_weave_moves_19(game: &Game) -> Vec<NodeId> {
    [
        node(game, Realm::Heaven, [1, 0]),
        node(game, Realm::Mortal, [1, 0]),
        node(game, Realm::Mortal, [0, 0]),
        node(game, Realm::Mortal, [-1, 1]),
        node(game, Realm::Mortal, [0, -1]),
        node(game, Realm::Underworld, [0, -1]),
    ]
    .to_vec()
}

fn dark_filler_moves_19(game: &Game) -> Vec<NodeId> {
    [
        node(game, Realm::Heaven, [-2, 1]),
        node(game, Realm::Heaven, [-1, -1]),
        node(game, Realm::Heaven, [-2, 2]),
        node(game, Realm::Heaven, [2, -2]),
        node(game, Realm::Heaven, [1, -2]),
        node(game, Realm::Heaven, [-1, 2]),
        node(game, Realm::Heaven, [0, -2]),
        node(game, Realm::Heaven, [0, 2]),
        node(game, Realm::Heaven, [2, -1]),
        node(game, Realm::Heaven, [-2, 0]),
    ]
    .to_vec()
}

// ------------------------------------------------------------- double weave

// -------------------------------------------------------------------- sever

#[test]
fn sever_removes_stone_and_consumes_charge() {
    let mut game = new_game(19, SEVER_V1);
    assert_eq!(game.state().sever_charges, [3, 3]);
    let target = node(&game, Realm::Mortal, [0, 0]);
    game.play(Move::Place(target)).unwrap(); // Light takes center
    game.play(Move::Sever(target)).unwrap(); // Dark severs it
    assert_eq!(game.state().occupant(target), None);
    assert_eq!(game.state().sever_charges, [3, 2]);
    // Light may retake the same node.
    game.play(Move::Place(target)).unwrap();
    assert_eq!(game.state().occupant(target), Some(Player::Light));
}

#[test]
fn sever_cannot_target_origins_or_own_stones() {
    let mut game = new_game(19, SEVER_V1);
    let light_origin = game.board().definition().origins_of(Player::Light)[0];
    let own = node(&game, Realm::Mortal, [0, 0]);
    game.play(Move::Place(own)).unwrap(); // Light
                                          // Dark cannot sever an origin.
    assert_eq!(
        game.validate(&Move::Sever(light_origin)),
        Err(RuleError::CannotSever(light_origin))
    );
    // Dark cannot sever an empty node.
    let empty = node(&game, Realm::Mortal, [1, 1]);
    assert_eq!(
        game.validate(&Move::Sever(empty)),
        Err(RuleError::CannotSever(empty))
    );
    game.play(Move::Place(node(&game, Realm::Mortal, [1, 1])))
        .unwrap(); // Dark places
                   // Light cannot sever once out of charges: burn all three.
    let t = node(&game, Realm::Mortal, [1, 1]);
    for _charge in 0..3 {
        game.play(Move::Sever(t)).unwrap(); // Light burns a charge
        game.play(Move::Place(t)).unwrap(); // Dark retakes
    }
    assert_eq!(game.state().sever_charges[0], 0);
    assert_eq!(
        game.validate(&Move::Sever(t)),
        Err(RuleError::SeverUnavailable)
    );
}

#[test]
fn sever_can_break_provisional_weave() {
    let mut game = new_game(19, SEVER_V1);
    let weave = light_weave_moves_19(&game);
    let filler = dark_filler_moves_19(&game);
    for (i, &w) in weave.iter().enumerate() {
        game.play(Move::Place(w)).unwrap();
        if i < weave.len() - 1 {
            game.play(Move::Place(filler[i])).unwrap();
        }
    }
    assert_eq!(game.state().pending_weave, Some(Player::Light));
    // Dark's response: sever the center link — the weave is broken and the
    // game continues.
    let center = node(&game, Realm::Mortal, [0, 0]);
    game.play(Move::Sever(center)).unwrap();
    assert!(game.result().is_none(), "sever must break the weave");
    assert_eq!(game.state().pending_weave, None);
    assert!(!game.has_realm_weave(Player::Light));
    // Light retakes the center → provisional again.
    game.play(Move::Place(center)).unwrap();
    assert_eq!(game.state().pending_weave, Some(Player::Light));
}

// ---------------------------------------------------------------- territory

#[test]
fn pass_is_illegal_in_classic_rules() {
    let game = new_game(19, realmweave_core::THREE_REALMS_V1);
    assert_eq!(game.validate(&Move::Pass), Err(RuleError::PassUnavailable));
}

// ------------------------------------------------------------ replay/serde

#[test]
fn variant_games_replay_deterministically() {
    #[allow(clippy::single_element_loop)]
    for ruleset in [SEVER_V1] {
        let mut game = new_game(19, ruleset);
        let a = node(&game, Realm::Mortal, [0, 0]);
        let b = node(&game, Realm::Mortal, [1, 0]);
        game.play(Move::Place(a)).unwrap();
        game.play(Move::Place(b)).unwrap();
        if ruleset == SEVER_V1 {
            game.play(Move::Sever(b)).unwrap();
        } else {
            game.play(Move::Place(node(&game, Realm::Mortal, [0, 1])))
                .unwrap();
        }
        let record = game.record();
        let replayed = Game::replay(board(19), record.config.clone(), &record.moves).unwrap();
        assert_eq!(replayed.state(), game.state(), "ruleset {ruleset}");
    }
}
