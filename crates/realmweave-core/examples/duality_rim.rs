//! v4 candidate measurement: origins as RIMS (whole board sides) instead of
//! corner points. Theory: Hex/Y duality comes from connecting SIDES — a
//! wall that blocks a side-to-side connection is itself a connected
//! structure. Point targets are why 53% of random fills are dead.
//!
//! Goal tested here: one connected group touching your rim in ALL THREE
//! realms (rims rotate 120° per realm, Dark point-reflected, like origins).
use realmweave_core::{boardgen, BoardGraph, NodeId, Player, Realm};

fn xorshift(x: &mut u64) -> u64 {
    *x ^= *x << 13; *x ^= *x >> 7; *x ^= *x << 17; *x
}

/// Outer-ring nodes of a realm whose axial coords lie on the side facing
/// angle `sector` (0..6). Side k of a hex of radius R: nodes with
/// distance R and "between corners k and k+1".
fn side_nodes(board: &BoardGraph, realm: Realm, sector: usize, radius: i32) -> Vec<NodeId> {
    // corners in axial coords, order matches DIRS hexagon
    let corners = [
        [radius, 0], [0, radius], [-radius, radius],
        [-radius, 0], [0, -radius], [radius, -radius],
    ];
    let a = corners[sector % 6];
    let b = corners[(sector + 1) % 6];
    // nodes on the line from a to b (inclusive): linear interpolation in cube coords
    let mut out = Vec::new();
    for t in 0..=radius {
        let q = a[0] + (b[0] - a[0]) * t / radius;
        let r = a[1] + (b[1] - a[1]) * t / radius;
        if let Some(&id) = board.axial_index().get(&(realm, [q, r])) {
            out.push(id);
        }
    }
    out
}

fn connects_all_rims(board: &BoardGraph, occ: &[Option<Player>], player: Player, rims: &[Vec<NodeId>; 3]) -> bool {
    // BFS over player's stones; must touch at least one node of each rim.
    let n = board.node_count();
    let mut visited = vec![false; n];
    let mut comp_touch = [false; 3];
    for start in 0..n as NodeId {
        if occ[start as usize] != Some(player) || visited[start as usize] { continue; }
        // BFS this component
        let mut queue = std::collections::VecDeque::new();
        let mut touch = [false; 3];
        visited[start as usize] = true;
        queue.push_back(start);
        let mut members = vec![start];
        while let Some(cur) = queue.pop_front() {
            for &nb in board.neighbors(cur) {
                if !visited[nb as usize] && occ[nb as usize] == Some(player) {
                    visited[nb as usize] = true;
                    queue.push_back(nb);
                    members.push(nb);
                }
            }
        }
        for m in members {
            for (k, rim) in rims.iter().enumerate() {
                if rim.contains(&m) { touch[k] = true; }
            }
        }
        if touch.iter().all(|&t| t) { return true; }
        for k in 0..3 { comp_touch[k] |= touch[k]; }
    }
    false
}

fn main() {
    let size: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(37);
    let trials: u32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let radius = match size { 19 => 2, 37 => 3, 61 => 4, 91 => 5, 127 => 6, _ => 3 };
    let def = boardgen::generate_standard(size).unwrap();
    let board = BoardGraph::new(def).unwrap();
    let n = board.node_count();

    // Light rims: side 0 in Heaven, side 2 in Mortal, side 4 in Underworld.
    // Dark rims: point-reflected (side 3, 5, 1).
    let rims_l = [
        side_nodes(&board, Realm::Heaven, 0, radius),
        side_nodes(&board, Realm::Mortal, 2, radius),
        side_nodes(&board, Realm::Underworld, 4, radius),
    ];
    let rims_d = [
        side_nodes(&board, Realm::Heaven, 3, radius),
        side_nodes(&board, Realm::Mortal, 5, radius),
        side_nodes(&board, Realm::Underworld, 1, radius),
    ];

    let (mut both, mut only_l, mut only_d, mut neither) = (0u32, 0, 0, 0);
    let mut rng = 0x1234_5678_9ABC_DEF0u64;
    for _ in 0..trials {
        let mut occ: Vec<Option<Player>> = vec![None; n];
        let mut ids: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            let j = (xorshift(&mut rng) % (i as u64 + 1)) as usize;
            ids.swap(i, j);
        }
        for (k, &i) in ids.iter().enumerate() {
            occ[i] = Some(if k % 2 == 0 { Player::Light } else { Player::Dark });
        }
        let l = connects_all_rims(&board, &occ, Player::Light, &rims_l);
        let d = connects_all_rims(&board, &occ, Player::Dark, &rims_d);
        match (l, d) {
            (true, true) => both += 1,
            (true, false) => only_l += 1,
            (false, true) => only_d += 1,
            (false, false) => neither += 1,
        }
    }
    let t = trials as f64;
    println!("hex{size} RIM goal, random fill × {trials}:");
    println!("  exactly one: {:.1}%", 100.0 * (only_l + only_d) as f64 / t);
    println!("  both:        {:.1}%", 100.0 * both as f64 / t);
    println!("  neither:     {:.1}%", 100.0 * neither as f64 / t);
}
