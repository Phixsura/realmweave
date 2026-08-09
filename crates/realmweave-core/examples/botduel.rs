//! Quick bot-vs-bot harness: geometry + outcome stats for tuning.
use realmweave_core::{boardgen, bot, BoardGraph, Game, GameConfig, Move};

fn main() {
    let games: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let size: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(61);
    for g in 0..games {
        let def = boardgen::generate_standard(size).unwrap();
        let board = BoardGraph::new(def).unwrap();
        let cfg = GameConfig::new(board.definition().id.clone())
            .with_ruleset(realmweave_core::WEAVE_LAYERS_V3);
        let mut game = Game::new(board, cfg).unwrap();
        let mut cuts = 0;
        while game.result().is_none() && game.state().ply < 700 {
            let seed = 0xD0E1u64
                .wrapping_add(g as u64 * 0x9E37)
                .wrapping_add(game.state().ply as u64);
            let mv = bot::choose_move(&game, seed).unwrap_or(Move::Pass);
            if matches!(mv, Move::CutEdge(_)) {
                cuts += 1;
            }
            if game.play(mv).is_err() {
                let _ = game.play(Move::Pass);
            }
        }
        // straightness metric: mean pairwise-adjacent turn count of Light's stones is hard;
        // use realm spread + stone clustering instead
        let st = game.state();
        let n_moves = st.move_log.len();
        // branchiness: average #same-color neighbors of each stone (line=2, web>2)
        let bd = game.board();
        let mut deg_sum = 0.0f64;
        let mut stones = 0.0f64;
        for (nd, occ) in st.occupancy.iter().enumerate() {
            if let Some(pl) = occ {
                let d = bd
                    .live_neighbors(nd as u16, &st.cut_edges)
                    .filter(|&nb| st.occupant(nb) == Some(*pl))
                    .count();
                deg_sum += d as f64;
                stones += 1.0;
            }
        }
        println!(
            "game {}: {:?} moves={} cuts={} layers={:?} mean_same_deg={:.2}",
            g,
            game.result(),
            n_moves,
            cuts,
            st.layers,
            deg_sum / stones.max(1.0)
        );
    }
}
