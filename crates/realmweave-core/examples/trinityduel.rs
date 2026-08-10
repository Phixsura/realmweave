//! Trinity Y bot self-play: decisiveness, length, realm-lead changes.
use realmweave_core::rules::TrinityY;
use realmweave_core::{boardgen, bot, BoardGraph, Game, GameConfig};

fn main() {
    let games: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let side: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(11);
    for g in 0..games {
        let def = boardgen::generate_trinity(side).unwrap();
        let board = BoardGraph::new(def).unwrap();
        let cfg = GameConfig::new(board.definition().id.clone())
            .with_ruleset(realmweave_core::TRINITY_Y_V4);
        let mut game = Game::new(board, cfg).unwrap();
        let mut events = Vec::new();
        let mut last = [0u8; 2];
        while game.result().is_none() {
            let seed = (0xD0E1u64 ^ (g as u64).wrapping_mul(0xA5A5_5A5A_1234_5678))
                .wrapping_add(g as u64 * 0x9E37)
                .wrapping_add(game.state().ply as u64);
            let Some(mv) = bot::choose_move(&game, seed) else {
                break;
            };
            if game.play(mv).is_err() {
                break;
            }
            if game.state().layers != last {
                events.push((game.state().ply, game.state().layers));
                last = game.state().layers;
            }
        }
        let _ = TrinityY::realm_scores(game.board(), game.state());
        println!(
            "g{g}: {:?} moves={} captures={:?} realm_events={:?}",
            game.result(),
            game.state().move_log.len(),
            game.state().captures,
            events
        );
    }
}
