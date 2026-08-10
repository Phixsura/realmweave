//! v4 geometry hypothesis: PORTAL SCARCITY is the deadness source.
//! Weld the three realms along their entire rims (every outer node gets a
//! portal column) → the world becomes one closed surface, and side-to-side
//! duality has a chance to survive realm crossings.
//! Sweep portal density: 12 gates (current) → full rim weld.
use realmweave_core::board::{Edge, EdgeKind};
use realmweave_core::rules::live_realm_weave;
use realmweave_core::{boardgen, BoardGraph, GameState, Player, Realm};

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
    for weld in [0usize, 1, 2, 3] {
        // weld levels: 0 = standard gates only; 1 = + every 2nd outer node;
        // 2 = every outer node; 3 = ALL nodes (full columns everywhere)
        let mut def = boardgen::generate_standard(size).unwrap();
        let radius = match size {
            19 => 2,
            37 => 3,
            61 => 4,
            91 => 5,
            127 => 6,
            _ => 3,
        };
        let realm_size = def.nodes.len() / 3;
        let mut added = 0;
        for i in 0..realm_size {
            let node = &def.nodes[i];
            let Some(ax) = node.axial else { continue };
            let dist = (ax[0].abs() + ax[1].abs() + (ax[0] + ax[1]).abs()) / 2;
            let on_rim = dist == radius;
            let include = match weld {
                0 => false,
                1 => on_rim && (ax[0] - ax[1]).rem_euclid(2) == 0,
                2 => on_rim,
                _ => true,
            };
            if !include {
                continue;
            }
            // add portal H<->M and M<->U at this axial if not already present
            let h = i as u16;
            let m = (i + realm_size) as u16;
            let u = (i + 2 * realm_size) as u16;
            for (a, b) in [(h, m), (m, u)] {
                let exists = def
                    .edges
                    .iter()
                    .any(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a));
                if !exists {
                    def.edges.push(Edge {
                        a,
                        b,
                        kind: EdgeKind::Portal,
                    });
                    added += 1;
                }
            }
        }
        let board = BoardGraph::new(def).unwrap();
        let n = board.node_count();
        let origins_l = board.definition().origins_of(Player::Light);
        let origins_d = board.definition().origins_of(Player::Dark);
        let (mut both, mut one, mut neither) = (0u32, 0, 0);
        let mut rng = 0xDEAD_BEEF_1234_5678u64;
        for _ in 0..trials {
            let mut st = GameState::new(board.definition().id.clone(), n);
            let mut ids: Vec<usize> = (0..n).collect();
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
                (false, false) => neither += 1,
                _ => one += 1,
            }
        }
        let t = trials as f64;
        println!(
            "weld {weld} (+{added} portals): one {:.1}% | both {:.1}% | neither {:.1}%",
            100.0 * one as f64 / t,
            100.0 * both as f64 / t,
            100.0 * neither as f64 / t
        );
        let _ = Realm::ALL;
    }
}
