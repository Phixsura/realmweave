//! Straightness metric for trinity: longest straight chain of adjacent
//! same-color stones along each triangular direction, final position.
use realmweave_core::{boardgen, bot, BoardGraph, Game, GameConfig, Move, Player};

fn main() {
    let side = 14usize;
    let def = boardgen::generate_trinity(side).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let cfg =
        GameConfig::new(board.definition().id.clone()).with_ruleset(realmweave_core::TRINITY_Y_V4);
    let mut game = Game::new(board, cfg).unwrap();
    while game.result().is_none() && game.state().ply < 700 {
        let seed = 0xABCDu64.wrapping_add(game.state().ply as u64);
        let Some(mv) = bot::choose_move(&game, seed) else {
            break;
        };
        if game.play(mv).is_err() {
            let _ = game.play(Move::Pass);
        }
    }
    let st = game.state();
    let bd = game.board();
    let per = side * (side + 1) / 2;
    let coord = |n: u16| -> (usize, i64, i64) {
        let local = n as usize % per;
        let r = ((((8 * local + 1) as f64).sqrt() - 1.0) / 2.0) as i64;
        let c = local as i64 - r * (r + 1) / 2;
        (n as usize / per, r, c)
    };
    let idx_of = |realm: usize, r: i64, c: i64| -> Option<u16> {
        if r < 0 || r >= side as i64 || c < 0 || c > r {
            return None;
        }
        Some((realm * per) as u16 + (r * (r + 1) / 2 + c) as u16)
    };
    const DIRS: [[i64; 2]; 3] = [[0, 1], [1, 0], [1, 1]]; // 3 axes
    for pl in [Player::Light, Player::Dark] {
        let mut longest = 0usize;
        for n in 0..bd.node_count() as u16 {
            if st.occupant(n) != Some(pl) {
                continue;
            }
            let (realm, r0, c0) = coord(n);
            for d in DIRS {
                let mut len = 1;
                let (mut r, mut c) = (r0, c0);
                while let Some(id) = idx_of(realm, r + d[0], c + d[1]) {
                    if st.occupant(id) == Some(pl) {
                        len += 1;
                        r += d[0];
                        c += d[1];
                    } else {
                        break;
                    }
                }
                longest = longest.max(len);
            }
        }
        println!(
            "{pl:?}: longest straight chain = {longest} (moves={} result={:?})",
            st.move_log.len(),
            game.result()
        );
    }
}
