//! How do v3 games on hex91 actually end?
use realmweave_core::{boardgen, bot, BoardGraph, Game, GameConfig, Move};
fn main() {
    let def = boardgen::generate_standard(91).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let cfg = GameConfig::new(board.definition().id.clone())
        .with_ruleset(realmweave_core::WEAVE_LAYERS_V3);
    let mut game = Game::new(board, cfg).unwrap();
    while game.result().is_none() && game.state().ply < 700 {
        let seed = 0xD0E1u64.wrapping_add(game.state().ply as u64);
        let mv = bot::choose_move(&game, seed).unwrap_or(Move::Pass);
        let _ = game.play(mv);
    }
    let st = game.state();
    println!(
        "result {:?} moves={} layers={:?}",
        game.result(),
        st.move_log.len(),
        st.layers
    );
    println!(
        "last 8: {:?}",
        &st.move_log[st.move_log.len().saturating_sub(8)..]
    );
    let empties = st
        .occupancy
        .iter()
        .enumerate()
        .filter(|(i, o)| o.is_none() && !st.is_petrified(*i as u16))
        .count();
    println!("empties left: {empties} / {}", st.occupancy.len());
}
