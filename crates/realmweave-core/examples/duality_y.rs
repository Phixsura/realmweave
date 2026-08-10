//! The Y insight: in the game of Y (triangle board, both players race to
//! connect ALL THREE sides), topology guarantees EXACTLY ONE player
//! succeeds on a full board — same-goal duality, no draws, attack=defense.
//! Test whether a Y-like goal survives on our geometry:
//!   G1: single hex realm, connect 3 alternating sides (shared goal)
//!   G2: three realms fully welded, connect the same 3 alternating sides
//!       but each side must be touched in a DIFFERENT realm (trinity Y)
//!   G3: three realms welded, plain Y on the merged surface (any realm)
use realmweave_core::board::{Edge, EdgeKind};
use realmweave_core::{boardgen, BoardGraph, NodeId, Player, Realm};

fn xorshift(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

fn side_nodes(board: &BoardGraph, realm: Realm, sector: usize, radius: i32) -> Vec<NodeId> {
    let corners = [
        [radius, 0],
        [0, radius],
        [-radius, radius],
        [-radius, 0],
        [0, -radius],
        [radius, -radius],
    ];
    let a = corners[sector % 6];
    let b = corners[(sector + 1) % 6];
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

/// One connected component of `player` touches all three side-sets.
fn y_connected(
    board: &BoardGraph,
    occ: &[Option<Player>],
    player: Player,
    sides: &[Vec<NodeId>; 3],
) -> bool {
    let n = board.node_count();
    let mut visited = vec![false; n];
    for start in 0..n as NodeId {
        if occ[start as usize] != Some(player) || visited[start as usize] {
            continue;
        }
        let mut queue = std::collections::VecDeque::new();
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
        let mut touch = [false; 3];
        for m in &members {
            for k in 0..3 {
                if sides[k].contains(m) {
                    touch[k] = true;
                }
            }
        }
        if touch.iter().all(|&t| t) {
            return true;
        }
    }
    false
}

fn run(
    board: &BoardGraph,
    sides_l: &[Vec<NodeId>; 3],
    sides_d: &[Vec<NodeId>; 3],
    trials: u32,
    label: &str,
) {
    let n = board.node_count();
    let (mut both, mut one, mut neither) = (0u32, 0, 0);
    let mut rng = 0x0BAD_F00D_CAFE_BABEu64;
    for _ in 0..trials {
        let mut occ: Vec<Option<Player>> = vec![None; n];
        let mut ids: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            let j = (xorshift(&mut rng) % (i as u64 + 1)) as usize;
            ids.swap(i, j);
        }
        for (k, &i) in ids.iter().enumerate() {
            occ[i] = Some(if k % 2 == 0 {
                Player::Light
            } else {
                Player::Dark
            });
        }
        let l = y_connected(board, &occ, Player::Light, sides_l);
        let d = y_connected(board, &occ, Player::Dark, sides_d);
        match (l, d) {
            (true, true) => both += 1,
            (false, false) => neither += 1,
            _ => one += 1,
        }
    }
    let t = trials as f64;
    println!(
        "{label}: one {:.1}% | both {:.1}% | neither {:.1}%",
        100.0 * one as f64 / t,
        100.0 * both as f64 / t,
        100.0 * neither as f64 / t
    );
}

fn welded(size: usize) -> BoardGraph {
    let mut def = boardgen::generate_standard(size).unwrap();
    let realm_size = def.nodes.len() / 3;
    for i in 0..realm_size {
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
            }
        }
    }
    BoardGraph::new(def).unwrap()
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
    let radius = match size {
        19 => 2,
        37 => 3,
        61 => 4,
        91 => 5,
        127 => 6,
        _ => 3,
    };

    // G1: single realm (Heaven only board = just use Heaven side sets and
    // ignore other realms by filling them too — component search is global
    // but sides are all in Heaven, so realm crossing irrelevant on standard
    // gates... to be clean, test on the standard board but sides in Heaven).
    let std_board = BoardGraph::new(boardgen::generate_standard(size).unwrap()).unwrap();
    let g1 = [
        side_nodes(&std_board, Realm::Heaven, 0, radius),
        side_nodes(&std_board, Realm::Heaven, 2, radius),
        side_nodes(&std_board, Realm::Heaven, 4, radius),
    ];
    run(
        &std_board,
        &g1,
        &g1,
        trials,
        "G1 single-realm Y (shared sides 0/2/4)",
    );

    // G2: welded board, trinity Y — side 0 in Heaven, side 2 in Mortal,
    // side 4 in Underworld, SHARED by both players.
    let wb = welded(size);
    let g2 = [
        side_nodes(&wb, Realm::Heaven, 0, radius),
        side_nodes(&wb, Realm::Mortal, 2, radius),
        side_nodes(&wb, Realm::Underworld, 4, radius),
    ];
    run(
        &wb,
        &g2,
        &g2,
        trials,
        "G2 welded trinity-Y (H0/M2/U4 shared)",
    );

    // G3: welded board, Y where each side-set = that side in ALL realms.
    let g3 = [
        [
            side_nodes(&wb, Realm::Heaven, 0, radius),
            side_nodes(&wb, Realm::Mortal, 0, radius),
            side_nodes(&wb, Realm::Underworld, 0, radius),
        ]
        .concat(),
        [
            side_nodes(&wb, Realm::Heaven, 2, radius),
            side_nodes(&wb, Realm::Mortal, 2, radius),
            side_nodes(&wb, Realm::Underworld, 2, radius),
        ]
        .concat(),
        [
            side_nodes(&wb, Realm::Heaven, 4, radius),
            side_nodes(&wb, Realm::Mortal, 4, radius),
            side_nodes(&wb, Realm::Underworld, 4, radius),
        ]
        .concat(),
    ];
    run(&wb, &g3, &g3, trials, "G3 welded Y (side k = all realms)");
}
