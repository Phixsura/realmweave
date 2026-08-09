//! Property-based tests: replay equivalence, serialization round trips,
//! move legality invariants, and occupancy invariants.

use proptest::prelude::*;
use realmweave_core::board::BoardGraph;
use realmweave_core::boardgen;
use realmweave_core::{Game, GameConfig, GameState, Move, NodeId};

fn board(size: usize) -> BoardGraph {
    BoardGraph::new(boardgen::generate_standard(size).unwrap()).unwrap()
}

/// Play a deterministic game driven by a seed sequence: each u16 picks the
/// n-th legal placement (mod count). Returns the finished-or-exhausted game.
fn drive_game(size: usize, picks: &[u16], pie: bool) -> Game {
    let b = board(size);
    let config = GameConfig::new(b.definition().id.clone()).with_pie_rule(pie);
    let mut game = Game::new(b, config).unwrap();
    for &pick in picks {
        if game.result().is_some() {
            break;
        }
        let placements: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m, Move::Place(_) | Move::Swap))
            .collect();
        if placements.is_empty() {
            break;
        }
        let mv = placements[pick as usize % placements.len()];
        game.play(mv).unwrap();
    }
    game
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Replaying any game's move log reproduces the identical state.
    #[test]
    fn replay_equivalence(picks in prop::collection::vec(0u16..500, 0..80), pie in any::<bool>()) {
        let game = drive_game(19, &picks, pie);
        let record = game.record();
        let replayed = Game::replay(board(19), record.config, &record.moves).unwrap();
        prop_assert_eq!(replayed.state(), game.state());
    }

    /// GameState serialization round-trips exactly.
    #[test]
    fn state_serde_round_trip(picks in prop::collection::vec(0u16..500, 0..60)) {
        let game = drive_game(19, &picks, false);
        let json = serde_json::to_string(game.state()).unwrap();
        let back: GameState = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(game.state(), &back);
    }

    /// Every legal move validates and applies; every occupied node is
    /// rejected; occupancy count matches ply arithmetic.
    #[test]
    fn legality_and_occupancy_invariants(picks in prop::collection::vec(0u16..500, 0..80)) {
        let game = drive_game(19, &picks, false);
        let state = game.state();

        // No duplicate occupancy is representable (Vec<Option<Player>>),
        // but check stone conservation: total stones = origins + placements.
        let placements = state.move_log.iter()
            .filter(|m| matches!(m, Move::Place(_)))
            .count();
        let stones = state.occupancy.iter().filter(|o| o.is_some()).count();
        prop_assert_eq!(stones, 6 + placements);

        if game.result().is_none() {
            for mv in game.legal_moves() {
                prop_assert!(game.validate(&mv).is_ok());
                if let Move::Place(n) = mv {
                    prop_assert!(state.occupant(n).is_none());
                }
            }
            // All occupied nodes rejected.
            for n in 0..game.board().node_count() as NodeId {
                if state.occupant(n).is_some() {
                    prop_assert!(game.validate(&Move::Place(n)).is_err());
                }
            }
        } else {
            prop_assert!(game.legal_moves().is_empty());
        }
    }

    /// Components partition the player's stones; weave detection agrees with
    /// component membership of origins.
    #[test]
    fn component_invariants(picks in prop::collection::vec(0u16..500, 0..80)) {
        let game = drive_game(19, &picks, false);
        for player in [realmweave_core::Player::Light, realmweave_core::Player::Dark] {
            let components = game.player_components(player);
            let mut all: Vec<NodeId> = components.iter().flatten().copied().collect();
            all.sort_unstable();
            let mut stones = game.state().stones_of(player);
            stones.sort_unstable();
            prop_assert_eq!(&all, &stones, "components must partition stones");

            let origins = game.board().definition().origins_of(player);
            let weave = components.iter().any(|c| {
                origins.iter().all(|o| c.binary_search(o).is_ok())
            });
            prop_assert_eq!(weave, game.has_realm_weave(player));
        }
    }
}
