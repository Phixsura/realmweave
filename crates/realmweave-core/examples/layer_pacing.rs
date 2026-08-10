use realmweave_core::{boardgen, bot, BoardGraph, Game, GameConfig, Move};
fn main() {
    let size: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(61);
    for g in 0..3u64 {
        let def = boardgen::generate_standard(size).unwrap();
        let board = BoardGraph::new(def).unwrap();
        let cfg = GameConfig::new(board.definition().id.clone())
            .with_ruleset(realmweave_core::WEAVE_LAYERS_V3);
        let mut game = Game::new(board, cfg).unwrap();
        let mut events = Vec::new();
        let mut last = [0u8; 2];
        while game.result().is_none() && game.state().ply < 700 {
            let seed = (0x77 + g * 0x9E37).wrapping_add(game.state().ply as u64);
            let mv = bot::choose_move(&game, seed).unwrap_or(Move::Pass);
            let _ = game.play(mv);
            if game.state().layers != last {
                events.push((game.state().ply, game.state().layers));
                last = game.state().layers;
            }
        }
        println!(
            "g{g}: {:?} total={} events={:?}",
            game.result(),
            game.state().move_log.len(),
            events
        );
    }
}
