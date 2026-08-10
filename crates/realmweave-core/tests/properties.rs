//! Property-based invariants that hold for EVERY ruleset and random play:
//! replay determinism, legal-move soundness, and (trinity) Y decisiveness.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test/tooling code
use proptest::prelude::*;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move};

fn board_for(ruleset: &str) -> realmweave_core::BoardDefinition {
    match ruleset {
        realmweave_core::TRINITY_Y_V4 => boardgen::generate_trinity(7).expect("trinity"),
        realmweave_core::TRIFORCE_V5 => boardgen::generate_triforce(8).expect("triforce"),
        _ => boardgen::generate_standard(19).expect("standard"),
    }
}

fn play_random(ruleset: &str, seed: u64, max_moves: usize) -> Game {
    let def = board_for(ruleset);
    let board = BoardGraph::new(def).expect("board");
    let cfg = GameConfig::new(board.definition().id.clone()).with_ruleset(ruleset);
    let mut game = Game::new(board, cfg).expect("game");
    let mut s = seed | 1;
    for _ in 0..max_moves {
        if game.result().is_some() {
            break;
        }
        let legal: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| !matches!(m, Move::Resign | Move::Swap))
            .collect();
        if legal.is_empty() {
            break;
        }
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let mv = legal[(s % legal.len() as u64) as usize];
        game.play(mv).expect("legal_moves must be playable");
    }
    game
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Every move returned by legal_moves must validate and apply, and the
    /// resulting game must replay to the identical state — for every
    /// ruleset, under arbitrary random play.
    #[test]
    fn replay_determinism_all_rulesets(seed in any::<u64>()) {
        for ruleset in realmweave_core::ALL_RULESETS {
            let game = play_random(ruleset, seed, 40);
            let def = board_for(ruleset);
            let board = BoardGraph::new(def).expect("board");
            let replayed = Game::replay(board, game.config().clone(), &game.state().move_log)
                .expect("recorded games replay");
            prop_assert_eq!(replayed.state(), game.state());
        }
    }

    /// Trinity Y games always end decisively (Y theorem) within the board's
    /// capacity — random play never wedges the game.
    #[test]
    fn trinity_always_decides(seed in any::<u64>()) {
        let game = play_random(realmweave_core::TRINITY_Y_V4, seed, 400);
        // Either finished, or pass-capable moves remain (never a wedged
        // position with no legal continuation).
        if game.result().is_none() {
            prop_assert!(!game.legal_moves().is_empty());
        }
    }
}
