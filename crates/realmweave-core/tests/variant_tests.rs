//! Tests for the doubleweave, sever, and territory rule variants.

use realmweave_core::board::BoardGraph;
use realmweave_core::boardgen;
use realmweave_core::rules::RuleError;
use realmweave_core::{
    Game, GameConfig, GameResult, Move, NodeId, Player, Realm, WinReason, DOUBLE_WEAVE_V1,
    SEVER_V1, TERRITORY_V1,
};

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

#[test]
fn single_route_is_not_enough_for_doubleweave() {
    let mut game = new_game(19, DOUBLE_WEAVE_V1);
    let weave = light_weave_moves_19(&game);
    let filler = dark_filler_moves_19(&game);
    for (i, &w) in weave.iter().enumerate() {
        game.play(Move::Place(w)).unwrap();
        if i < weave.len() - 1 {
            game.play(Move::Place(filler[i])).unwrap();
        }
    }
    // Classic rules would flag a provisional weave here; doubleweave demands
    // a second disjoint route.
    assert!(game.has_realm_weave(Player::Light), "single route exists");
    assert_eq!(game.state().pending_weave, None, "one route must not count");
    assert!(game.result().is_none());
}

#[test]
fn two_disjoint_routes_win_doubleweave() {
    let mut game = new_game(19, DOUBLE_WEAVE_V1);
    // Light origins: H[2,0], M[-2,2], U[0,-2]. Build a 2-connected network:
    // every origin pair gets two internally-vertex-disjoint paths.
    //
    // Path pair H↔M:  Horig–H[1,0]–M[1,0]–M[0,0]–M[-1,1]–Morig
    //            and  Horig–H[1,1]–H[0,1]–M[0,1]–M[-1,2]–Morig
    // Path pair H↔U:  Horig–H[1,0]–M[1,0]–M[0,0]–M[0,-1]–U[0,-1]–Uorig
    //            and  Horig–H[2,-1]–H[1,-1]–M[1,-1]–U[1,-1]–U[1,-2]–Uorig
    // Path pair M↔U:  Morig–M[-1,1]–M[0,0]–M[0,-1]–U[0,-1]–Uorig
    //            and  Morig–M[-1,2]–M[0,1]–M[1,0]–M[1,-1]–U[1,-1]–U[1,-2]–Uorig
    let light: Vec<NodeId> = [
        (Realm::Heaven, [1, 0]),
        (Realm::Heaven, [1, 1]),
        (Realm::Heaven, [0, 1]),
        (Realm::Heaven, [2, -1]),
        (Realm::Heaven, [1, -1]),
        (Realm::Mortal, [1, 0]),
        (Realm::Mortal, [0, 0]),
        (Realm::Mortal, [-1, 1]),
        (Realm::Mortal, [0, -1]),
        (Realm::Mortal, [0, 1]),
        (Realm::Mortal, [-1, 2]),
        (Realm::Mortal, [1, -1]),
        (Realm::Underworld, [0, -1]),
        (Realm::Underworld, [1, -1]),
        (Realm::Underworld, [1, -2]),
    ]
    .into_iter()
    .map(|(r, ax)| node(&game, r, ax))
    .collect();
    // Dark fillers: all remaining Heaven nodes + far Underworld nodes.
    let dark: Vec<NodeId> = [
        (Realm::Heaven, [-2, 1]),
        (Realm::Heaven, [-2, 2]),
        (Realm::Heaven, [-1, -1]),
        (Realm::Heaven, [-1, 0]),
        (Realm::Heaven, [-1, 1]),
        (Realm::Heaven, [-1, 2]),
        (Realm::Heaven, [0, -2]),
        (Realm::Heaven, [0, -1]),
        (Realm::Heaven, [0, 0]),
        (Realm::Heaven, [0, 2]),
        (Realm::Heaven, [1, -2]),
        (Realm::Heaven, [2, -2]),
        (Realm::Underworld, [-1, 0]),
        (Realm::Underworld, [-2, 1]),
        (Realm::Underworld, [-1, 1]),
    ]
    .into_iter()
    .map(|(r, ax)| node(&game, r, ax))
    .collect();

    let mut di = 0;
    for &w in &light {
        if game.result().is_some() {
            break;
        }
        game.play(Move::Place(w)).unwrap(); // Light
        if game.result().is_none() {
            game.play(Move::Place(dark[di])).unwrap(); // Dark
            di += 1;
        }
    }
    // If the weave completed on Light's last stone, Dark still owes the
    // response turn.
    if game.result().is_none() && game.state().pending_weave == Some(Player::Light) {
        game.play(Move::Place(dark[di])).unwrap();
    }
    assert_eq!(
        game.result(),
        Some(GameResult::Win {
            player: Player::Light,
            reason: WinReason::RealmWeave
        })
    );
}

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
fn two_passes_end_game_with_territory_scoring() {
    let mut game = new_game(19, TERRITORY_V1);
    // Light builds a 3-stone chain from an origin; Dark places 2 isolated
    // stones far apart, then both pass.
    let l1 = node(&game, Realm::Mortal, [-1, 1]); // adj to M origin [-2,2]
    let l2 = node(&game, Realm::Mortal, [0, 0]);
    let l3 = node(&game, Realm::Mortal, [1, 0]);
    let d1 = node(&game, Realm::Heaven, [0, 2]);
    let d2 = node(&game, Realm::Underworld, [2, -2]);
    game.play(Move::Place(l1)).unwrap();
    game.play(Move::Place(d1)).unwrap();
    game.play(Move::Place(l2)).unwrap();
    game.play(Move::Place(d2)).unwrap();
    game.play(Move::Place(l3)).unwrap();
    game.play(Move::Pass).unwrap(); // Dark passes
    assert!(game.result().is_none(), "one pass must not end the game");
    game.play(Move::Pass).unwrap(); // Light passes → scored
                                    // Light: origin + 3 chain = component of 4. Dark components are all 1.
    assert_eq!(
        game.result(),
        Some(GameResult::Win {
            player: Player::Light,
            reason: WinReason::Territory
        })
    );
}

#[test]
fn pass_is_illegal_in_classic_rules() {
    let game = new_game(19, realmweave_core::THREE_REALMS_V1);
    assert_eq!(game.validate(&Move::Pass), Err(RuleError::PassUnavailable));
}

#[test]
fn weave_bonus_beats_bigger_blob() {
    let mut game = new_game(19, TERRITORY_V1);
    // Light connects all three origins (weave = +15 bonus, component 6+3).
    let weave = light_weave_moves_19(&game);
    // Dark builds one big 8-stone blob in Heaven (component 8+origin? Dark's
    // Heaven origin is [-2,0]; build adjacent chain).
    let dark_blob = [
        node(&game, Realm::Heaven, [-1, 0]),
        node(&game, Realm::Heaven, [-1, 1]),
        node(&game, Realm::Heaven, [0, 1]),
        node(&game, Realm::Heaven, [-1, -1]),
        node(&game, Realm::Heaven, [0, -1]),
        node(&game, Realm::Heaven, [-2, 1]),
    ];
    for (i, &w) in weave.iter().enumerate() {
        game.play(Move::Place(w)).unwrap();
        // Dark: place while blob nodes remain, then pass.
        let dark_mv = dark_blob
            .get(i)
            .map(|&n| Move::Place(n))
            .unwrap_or(Move::Pass);
        game.play(dark_mv).unwrap();
    }
    game.play(Move::Pass).unwrap(); // Light passes
    game.play(Move::Pass).unwrap(); // Dark passes → scored
                                    // Light: 6 origins-connected stones + 3 origins = 9 + 15 bonus = 24.
                                    // Dark: ~7-stone component, no weave.
    assert_eq!(
        game.result(),
        Some(GameResult::Win {
            player: Player::Light,
            reason: WinReason::Territory
        })
    );
}

// ------------------------------------------------------------ replay/serde

#[test]
fn variant_games_replay_deterministically() {
    for ruleset in [DOUBLE_WEAVE_V1, SEVER_V1, TERRITORY_V1] {
        let mut game = new_game(19, ruleset);
        let a = node(&game, Realm::Mortal, [0, 0]);
        let b = node(&game, Realm::Mortal, [1, 0]);
        game.play(Move::Place(a)).unwrap();
        game.play(Move::Place(b)).unwrap();
        if ruleset == SEVER_V1 {
            game.play(Move::Sever(b)).unwrap();
        } else if ruleset == TERRITORY_V1 {
            game.play(Move::Pass).unwrap();
        } else {
            game.play(Move::Place(node(&game, Realm::Mortal, [0, 1])))
                .unwrap();
        }
        let record = game.record();
        let replayed = Game::replay(board(19), record.config.clone(), &record.moves).unwrap();
        assert_eq!(replayed.state(), game.state(), "ruleset {ruleset}");
    }
}
