//! Rules, weave, pie rule, replay, and serialization tests.

use realmweave_core::board::BoardGraph;
use realmweave_core::boardgen;
use realmweave_core::rules::RuleError;
use realmweave_core::{
    Game, GameConfig, GameRecord, GameResult, Move, NodeId, Player, Realm, WinReason,
};

fn board(size: usize) -> BoardGraph {
    BoardGraph::new(boardgen::generate_standard(size).unwrap()).unwrap()
}

fn new_game(size: usize) -> Game {
    let b = board(size);
    let config = GameConfig::new(b.definition().id.clone());
    Game::new(b, config).unwrap()
}

fn new_pie_game(size: usize) -> Game {
    let b = board(size);
    let config = GameConfig::new(b.definition().id.clone()).with_pie_rule(true);
    Game::new(b, config).unwrap()
}

/// Node id for an axial coordinate in a realm.
fn node(game: &Game, realm: Realm, ax: [i32; 2]) -> NodeId {
    game.board().axial_index()[&(realm, ax)]
}

fn first_empty(game: &Game) -> NodeId {
    game.legal_moves()
        .into_iter()
        .find_map(|m| match m {
            Move::Place(n) => Some(n),
            _ => None,
        })
        .unwrap()
}

/// A path of empty nodes connecting Light's three origins on the 19 board
/// (radius 2) via two gate columns.
///
/// Light origins: Heaven [2,0], Mortal [-2,2], Underworld [0,-2].
/// All six ring-1 nodes are gate columns on the 19 board.
fn light_weave_moves_19(game: &Game) -> Vec<NodeId> {
    [
        node(game, Realm::Heaven, [1, 0]), // adjacent to Heaven origin [2,0]; gate
        node(game, Realm::Mortal, [1, 0]), // portal landing in Mortal
        node(game, Realm::Mortal, [0, 0]), // center hub
        node(game, Realm::Mortal, [-1, 1]), // toward Mortal origin [-2,2]... adjacency: [-1,1]+[-1,1]=[-2,2] ✓
        node(game, Realm::Mortal, [0, -1]), // gate column toward Underworld
        node(game, Realm::Underworld, [0, -1]), // portal landing; adjacent to U origin [0,-2]
    ]
    .to_vec()
}

/// Heaven-side filler moves for Dark that never touch the weave path.
fn dark_filler_moves_19(game: &Game) -> Vec<NodeId> {
    [
        node(game, Realm::Heaven, [-2, 1]),
        node(game, Realm::Heaven, [-1, -1]),
        node(game, Realm::Heaven, [-2, 2]),
        node(game, Realm::Heaven, [2, -2]),
        node(game, Realm::Heaven, [1, -2]),
        node(game, Realm::Heaven, [-1, 2]),
    ]
    .to_vec()
}

#[test]
fn origins_are_preoccupied() {
    let game = new_game(37);
    for origin in &game.board().definition().origins {
        assert_eq!(game.state().occupant(origin.node), Some(origin.player));
    }
}

#[test]
fn legal_placement_and_alternation() {
    let mut game = new_game(37);
    assert_eq!(game.to_move(), Player::Light);
    let empty = first_empty(&game);
    game.play(Move::Place(empty)).unwrap();
    assert_eq!(game.state().occupant(empty), Some(Player::Light));
    assert_eq!(game.to_move(), Player::Dark);
}

#[test]
fn rejects_occupied_and_unknown_nodes() {
    let mut game = new_game(37);
    let origin = game.board().definition().origins[0].node;
    assert_eq!(
        game.validate(&Move::Place(origin)),
        Err(RuleError::Occupied(origin))
    );
    let bogus = game.board().node_count() as NodeId;
    assert_eq!(
        game.validate(&Move::Place(bogus)),
        Err(RuleError::NoSuchNode(bogus))
    );
    assert!(game.play(Move::Place(origin)).is_err());
}

#[test]
fn cross_realm_connectivity_through_portals_only() {
    // Two Light stones on the same gate column in adjacent realms connect.
    let mut game = new_game(19);
    let heaven_gate = node(&game, Realm::Heaven, [1, 0]);
    let mortal_gate = node(&game, Realm::Mortal, [1, 0]);
    let dark_far = node(&game, Realm::Underworld, [-1, 0]);
    game.play(Move::Place(heaven_gate)).unwrap();
    game.play(Move::Place(dark_far)).unwrap();
    game.play(Move::Place(mortal_gate)).unwrap();
    let comp = game.connected_component(Player::Light, heaven_gate);
    assert!(comp.contains(&mortal_gate));

    // Vertically aligned stones on a NON-gate column must not connect.
    // On the 19 board, ring-2 node [2,-1] is not a gate (gates are the six
    // ring-1 nodes).
    let mut game2 = new_game(19);
    let h = node(&game2, Realm::Heaven, [2, -1]);
    let m = node(&game2, Realm::Mortal, [2, -1]);
    let filler = node(&game2, Realm::Underworld, [-1, 0]);
    game2.play(Move::Place(h)).unwrap();
    game2.play(Move::Place(filler)).unwrap();
    game2.play(Move::Place(m)).unwrap();
    let comp = game2.connected_component(Player::Light, h);
    assert!(!comp.contains(&m), "no portal at [2,-1]");
}

#[test]
fn enemy_stones_block_routes() {
    let mut game = new_game(19);
    let a = node(&game, Realm::Mortal, [1, 0]);
    let blocker = node(&game, Realm::Mortal, [0, 0]);
    let c = node(&game, Realm::Mortal, [-1, 0]);
    game.play(Move::Place(a)).unwrap(); // Light
    game.play(Move::Place(blocker)).unwrap(); // Dark takes center
    game.play(Move::Place(c)).unwrap(); // Light
    let comp = game.connected_component(Player::Light, a);
    assert!(!comp.contains(&c), "center blocker must sever the route");
}

#[test]
fn provisional_weave_then_confirmed_win() {
    let mut game = new_game(19);
    let weave = light_weave_moves_19(&game);
    let filler = dark_filler_moves_19(&game);
    for (i, &w) in weave.iter().enumerate() {
        assert!(game.result().is_none(), "game ended early at move {i}");
        game.play(Move::Place(w)).unwrap(); // Light
        if i < weave.len() - 1 {
            game.play(Move::Place(filler[i])).unwrap(); // Dark filler
        }
    }
    // Light just completed the weave: provisional, not yet a win.
    assert!(game.has_realm_weave(Player::Light));
    assert_eq!(game.state().pending_weave, Some(Player::Light));
    assert!(game.result().is_none(), "weave must not win immediately");

    // Opponent gets one full turn to respond.
    game.play(Move::Place(filler[weave.len() - 1])).unwrap();

    // Weave survived → confirmed at the start of Light's next turn.
    assert_eq!(
        game.result(),
        Some(GameResult::Win {
            player: Player::Light,
            reason: WinReason::RealmWeave
        })
    );
    assert!(game.legal_moves().is_empty());
}

/// In V1 stones are permanent, so a *completed* weave cannot physically be
/// severed; the confirmation turn exists for future sever/capture variants
/// and to make win timing explicit. What CAN happen is that the opponent
/// prevents completion by occupying the last connecting node first — verify
/// blocking the weave before it exists.
#[test]
fn opponent_blocks_weave_completion() {
    let mut game = new_game(19);
    let weave = light_weave_moves_19(&game);
    let filler = dark_filler_moves_19(&game);
    // Light plays all but the final node; Dark plays fillers, then steals
    // the final weave node.
    for (i, &w) in weave.iter().take(weave.len() - 1).enumerate() {
        game.play(Move::Place(w)).unwrap();
        if i < weave.len() - 2 {
            game.play(Move::Place(filler[i])).unwrap();
        }
    }
    // Dark occupies Light's final connector.
    let last = *weave.last().unwrap();
    game.play(Move::Place(last)).unwrap(); // Dark's move
    assert_eq!(game.state().occupant(last), Some(Player::Dark));
    assert!(!game.has_realm_weave(Player::Light));
    assert_eq!(game.state().pending_weave, None);
    assert!(game.result().is_none());
}

#[test]
fn resignation_ends_game() {
    let mut game = new_game(37);
    game.play(Move::Resign).unwrap();
    assert_eq!(
        game.result(),
        Some(GameResult::Win {
            player: Player::Dark,
            reason: WinReason::Resignation
        })
    );
    assert!(game.play(Move::Place(0)).is_err());
}

#[test]
fn pie_rule_swap_available_only_after_first_move() {
    let mut game = new_pie_game(37);
    assert_eq!(game.validate(&Move::Swap), Err(RuleError::SwapUnavailable));
    game.play(Move::Place(first_empty(&game))).unwrap();
    // Dark's first response: swap is legal.
    assert!(game.legal_moves().contains(&Move::Swap));
    game.swap_sides().unwrap();
    // Swap consumed; Dark still owes a placement.
    assert_eq!(game.to_move(), Player::Dark);
    assert_eq!(game.validate(&Move::Swap), Err(RuleError::SwapUnavailable));
    game.play(Move::Place(first_empty(&game))).unwrap();
    assert_eq!(game.to_move(), Player::Light);
}

#[test]
fn pie_rule_swap_forfeited_if_not_taken_immediately() {
    let mut game = new_pie_game(37);
    game.play(Move::Place(first_empty(&game))).unwrap(); // Light
    game.play(Move::Place(first_empty(&game))).unwrap(); // Dark places instead
    assert_eq!(game.validate(&Move::Swap), Err(RuleError::SwapUnavailable));
}

#[test]
fn no_swap_without_pie_rule() {
    let mut game = new_game(37);
    game.play(Move::Place(first_empty(&game))).unwrap();
    assert!(!game.legal_moves().contains(&Move::Swap));
}

#[test]
fn undo_restores_previous_state() {
    let mut game = new_game(37);
    let snapshot = game.state().clone();
    game.play(Move::Place(first_empty(&game))).unwrap();
    game.undo().unwrap();
    assert_eq!(*game.state(), snapshot);
    assert!(game.undo().is_err());
}

#[test]
fn replay_reproduces_identical_state() {
    let mut game = new_game(19);
    let weave = light_weave_moves_19(&game);
    let filler = dark_filler_moves_19(&game);
    for (i, &w) in weave.iter().enumerate() {
        game.play(Move::Place(w)).unwrap();
        if i < weave.len() - 1 {
            game.play(Move::Place(filler[i])).unwrap();
        }
    }
    game.play(Move::Place(filler[weave.len() - 1])).unwrap();
    let record = game.record();
    assert!(record.result.is_some());

    let replayed = Game::replay(board(19), record.config.clone(), &record.moves).unwrap();
    assert_eq!(replayed.state(), game.state());
    assert_eq!(replayed.result(), game.result());
}

#[test]
fn record_serde_round_trip() {
    let mut game = new_pie_game(19);
    game.play(Move::Place(first_empty(&game))).unwrap();
    game.swap_sides().unwrap();
    let record = game.record();
    let json = serde_json::to_string(&record).unwrap();
    let back: GameRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record, back);
    let replayed = Game::replay(board(19), back.config, &back.moves).unwrap();
    assert_eq!(replayed.state(), game.state());
}

#[test]
fn state_serde_round_trip() {
    let mut game = new_game(19);
    let weave = light_weave_moves_19(&game);
    game.play(Move::Place(weave[0])).unwrap();
    let json = serde_json::to_string(game.state()).unwrap();
    let back: realmweave_core::GameState = serde_json::from_str(&json).unwrap();
    assert_eq!(*game.state(), back);
}

#[test]
fn complete_games_finish_on_all_sizes() {
    // Deterministic pseudo-random-ish full games: both players always take
    // the lowest-id empty node. Game must reach a result (weave or draw)
    // before the board fills, and never error.
    for size in [19, 37, 61] {
        let mut game = new_game(size);
        let mut plies = 0u32;
        while game.result().is_none() {
            let mv = game
                .legal_moves()
                .into_iter()
                .find(|m| matches!(m, Move::Place(_)))
                .expect("board not full without result implies a legal placement");
            game.play(mv).unwrap();
            plies += 1;
            assert!(
                plies <= (size * 3) as u32 + 2,
                "game exceeded board capacity"
            );
        }
        // Deterministic: replay produces the same result.
        let record = game.record();
        let replayed = Game::replay(board(size), record.config, &record.moves).unwrap();
        assert_eq!(replayed.result(), game.result());
    }
}
