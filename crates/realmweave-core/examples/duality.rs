//! Duality audit: random-fill the board, count outcomes.
//! Hex's guarantee = P(exactly one player weaves | full board) = 100%.
//! That number IS attack-defense unity. Measure ours.
use realmweave_core::rules::live_realm_weave;
use realmweave_core::{boardgen, BoardGraph, GameState, Player};

fn xorshift(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

fn main() {
    let size: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(37);
    let trials: u32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let def = boardgen::generate_standard(size).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let n = board.node_count();
    let origins_l = board.definition().origins_of(Player::Light);
    let origins_d = board.definition().origins_of(Player::Dark);

    let (mut both, mut only_l, mut only_d, mut neither) = (0u32, 0, 0, 0);
    let mut rng = 0x1234_5678_9ABC_DEF0u64;
    for _ in 0..trials {
        let mut st = GameState::new(board.definition().id.clone(), n);
        // random half-half fill (origins get their own color)
        let mut ids: Vec<usize> = (0..n).collect();
        // Fisher-Yates
        for i in (1..n).rev() {
            let j = (xorshift(&mut rng) % (i as u64 + 1)) as usize;
            ids.swap(i, j);
        }
        for (k, &i) in ids.iter().enumerate() {
            st.occupancy[i] = Some(if k % 2 == 0 {
                Player::Light
            } else {
                Player::Dark
            });
        }
        for &o in &origins_l {
            st.occupancy[o as usize] = Some(Player::Light);
        }
        for &o in &origins_d {
            st.occupancy[o as usize] = Some(Player::Dark);
        }
        let l = live_realm_weave(&board, &st, Player::Light);
        let d = live_realm_weave(&board, &st, Player::Dark);
        match (l, d) {
            (true, true) => both += 1,
            (true, false) => only_l += 1,
            (false, true) => only_d += 1,
            (false, false) => neither += 1,
        }
    }
    let t = trials as f64;
    println!("hex{size} full-board random fill × {trials}:");
    println!(
        "  exactly one weaves: {:.1}%  (Hex would be 100%)",
        100.0 * (only_l + only_d) as f64 / t
    );
    println!("  both weave:         {:.1}%", 100.0 * both as f64 / t);
    println!(
        "  NEITHER weaves:     {:.1}%  <- deadness: blocking exceeds building",
        100.0 * neither as f64 / t
    );
    println!(
        "  (L {:.1}% / D {:.1}%)",
        100.0 * only_l as f64 / t,
        100.0 * only_d as f64 / t
    );
}
