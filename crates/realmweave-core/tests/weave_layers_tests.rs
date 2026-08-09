//! Weave Layers v3: petrification, layer scoring, permanence-based strangle,
//! replay determinism. Design: docs/design-weave-layers-v3.md.

use realmweave_core::board::BoardGraph;
use realmweave_core::rules::RuleError;
use realmweave_core::{boardgen, Game, GameConfig, Move, NodeId, Player, Realm, WEAVE_LAYERS_V3};

fn new_game(size: usize) -> Game {
    let b = BoardGraph::new(boardgen::generate_standard(size).unwrap()).unwrap();
    let config = GameConfig::new(b.definition().id.clone()).with_ruleset(WEAVE_LAYERS_V3);
    Game::new(b, config).unwrap()
}

fn node(game: &Game, realm: Realm, ax: [i32; 2]) -> NodeId {
    game.board().axial_index()[&(realm, ax)]
}

/// Light's weave path on hex19 (same as the v2 tests).
fn light_weave_moves(game: &Game) -> Vec<NodeId> {
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

fn dark_filler_moves(game: &Game) -> Vec<NodeId> {
    [
        node(game, Realm::Heaven, [0, 2]),
        node(game, Realm::Heaven, [-1, -1]),
        node(game, Realm::Heaven, [1, -2]),
        node(game, Realm::Heaven, [2, -2]),
        node(game, Realm::Heaven, [0, -2]),
        node(game, Realm::Heaven, [-1, 2]),
        node(game, Realm::Heaven, [1, 1]),
    ]
    .to_vec()
}

/// Drive Light to a confirmed weave; Dark plays far-away filler.
fn score_first_layer(game: &mut Game) {
    let weave = light_weave_moves(game);
    let filler = dark_filler_moves(game);
    for (i, &w) in weave.iter().enumerate() {
        game.play(Move::Place(w)).unwrap();
        game.play(Move::Place(filler[i])).unwrap();
    }
    // Light's weave was provisional before Dark's last filler; Light's next
    // move confirms... confirmation happens ON Dark's reply. After the loop
    // pending may already be resolved; if not, one more exchange:
    if game.result().is_none() && game.state().layers == [0, 0] {
        // weave pending: Dark's filler above was the response; if layers
        // still 0, play one more Light move to re-form / confirm.
        game.play(Move::Pass).unwrap();
    }
}

#[test]
fn first_weave_scores_a_layer_and_continues() {
    let mut game = new_game(19);
    score_first_layer(&mut game);
    assert_eq!(game.state().layers, [1, 0], "Light scored one layer");
    assert!(game.result().is_none(), "game continues after one layer");
}

#[test]
fn petrified_network_blocks_and_protects() {
    let mut game = new_game(19);
    score_first_layer(&mut game);
    let st = game.state();
    // Some nodes are petrified; petrified nodes are unplaceable.
    let petrified: Vec<NodeId> = (0..game.board().node_count() as NodeId)
        .filter(|&n| st.is_petrified(n))
        .collect();
    assert!(!petrified.is_empty(), "the weave petrified");
    let target = petrified[0];
    assert!(
        game.validate(&Move::Place(target)).is_err(),
        "cannot place on world structure"
    );
    // Origin-adjacent weave stones were removed, not petrified: the Light
    // Heaven origin's neighbors must all be free of petrification.
    let ho = node(&game, Realm::Heaven, [2, 0]);
    for &nb in game.board().neighbors(ho) {
        assert!(
            !st.is_petrified(nb),
            "origin breathing room: {nb} must not petrify"
        );
    }
}

#[test]
fn scissors_replenish_on_layer() {
    let mut game = new_game(19);
    // Light spends one scissor early (any legal cut).
    let e = game
        .legal_moves()
        .into_iter()
        .find_map(|m| match m {
            Move::CutEdge(e) => Some(e),
            _ => None,
        })
        .expect("some cut is legal");
    game.play(Move::CutEdge(e)).unwrap(); // L: 3→2
    game.play(Move::Place(node(&game, Realm::Underworld, [-1, 0])))
        .unwrap(); // D
    assert_eq!(game.state().scissors[0], 2);
    score_first_layer(&mut game);
    // +2 capped at 4: Light 2+2=4, Dark 3+2 → capped 4.
    assert_eq!(game.state().scissors, [4, 4]);
}

#[test]
fn stones_alone_cannot_strangle_in_v3() {
    // The v2-style siege (wall stones enclosing Dark's Mortal origin
    // corner) must NOT end the game in v3 — enemy stones are not permanent
    // terrain, so strangle requires cuts/petrification.
    let mut game = new_game(19);
    let walls = [
        node(&game, Realm::Mortal, [0, 0]),
        node(&game, Realm::Mortal, [1, 0]),
        node(&game, Realm::Mortal, [0, -1]),
        node(&game, Realm::Mortal, [2, 0]),
        node(&game, Realm::Mortal, [0, -2]),
    ];
    let filler = dark_filler_moves(&game);
    for i in 0..5 {
        game.play(Move::Place(walls[i])).unwrap();
        game.play(Move::Place(filler[i])).unwrap();
    }
    assert!(
        game.result().is_none(),
        "a stone wall must not strangle in v3: {:?}",
        game.result()
    );
}

#[test]
fn radius_two_sanctum_on_larger_boards() {
    let game = new_game(37);
    // A node at distance 2 from an enemy origin is unplaceable on hex37.
    let dark_origin = game.board().definition().origins_of(Player::Dark)[0];
    let d1 = game.board().neighbors(dark_origin)[0];
    let d2 = *game
        .board()
        .neighbors(d1)
        .iter()
        .find(|&&n| n != dark_origin && !game.board().neighbors(dark_origin).contains(&n))
        .unwrap();
    assert_eq!(
        game.validate(&Move::Place(d2)),
        Err(RuleError::OriginSanctum(d2))
    );
}

#[test]
fn portal_edges_uncuttable_in_v3() {
    let game = new_game(19);
    let def = game.board().definition();
    let portal = def
        .edges
        .iter()
        .position(|e| e.kind == realmweave_core::EdgeKind::Portal)
        .unwrap() as u32;
    assert!(
        game.validate(&Move::CutEdge(portal)).is_err(),
        "portals are the world's skeleton"
    );
}

#[test]
fn replay_reproduces_layers_and_petrification() {
    let mut game = new_game(19);
    score_first_layer(&mut game);
    let log = game.state().move_log.clone();
    let b = BoardGraph::new(boardgen::generate_standard(19).unwrap()).unwrap();
    let replayed = Game::replay(b, game.config().clone(), &log).unwrap();
    assert_eq!(replayed.state(), game.state());
}

#[test]
fn second_layer_needs_a_new_route() {
    let mut game = new_game(19);
    score_first_layer(&mut game);
    // The exact nodes of layer 1 are gone (petrified or removed): replaying
    // the same weave placements must be at least partially illegal.
    let weave = light_weave_moves(&game);
    let blocked = weave
        .iter()
        .filter(|&&n| game.validate(&Move::Place(n)).is_err())
        .count();
    assert!(
        blocked > 0,
        "petrification must invalidate part of the old route"
    );
}
