//! THE CANDIDATE: realms as TRIANGLES, goal = game-of-Y per realm.
//! Y's theorem: on a full triangular board exactly ONE player connects all
//! three sides — attack IS defense, no draws, no patches.
//! v4 hypothesis: three triangular realms, portals couple them, match won
//! by taking 2 of 3 realms. Per-realm decisiveness = 100% by topology, so
//! the match is ALWAYS decisive. Measure to confirm + measure coupling.
use std::collections::VecDeque;

fn xorshift(x: &mut u64) -> u64 { *x ^= *x << 13; *x ^= *x >> 7; *x ^= *x << 17; *x }

/// Triangular grid with side N: rows 0..N, row r has r+1 cells.
/// Cell (r, c). Neighbors: (r,c-1),(r,c+1),(r-1,c-1),(r-1,c),(r+1,c),(r+1,c+1).
struct Tri {
    n: usize,
    index: Vec<Vec<usize>>, // [r][c] -> id
    coords: Vec<(usize, usize)>,
}

impl Tri {
    fn new(n: usize) -> Self {
        let mut index = Vec::new();
        let mut coords = Vec::new();
        for r in 0..n {
            let mut row = Vec::new();
            for c in 0..=r {
                row.push(coords.len());
                coords.push((r, c));
            }
            index.push(row);
        }
        Tri { n, index, coords }
    }
    fn cells(&self) -> usize { self.coords.len() }
    fn neighbors(&self, id: usize) -> Vec<usize> {
        let (r, c) = self.coords[id];
        let mut out = Vec::new();
        let get = |rr: i64, cc: i64| -> Option<usize> {
            if rr < 0 || rr >= self.n as i64 || cc < 0 || cc > rr { None }
            else { Some(self.index[rr as usize][cc as usize]) }
        };
        let (r, c) = (r as i64, c as i64);
        for (rr, cc) in [(r, c - 1), (r, c + 1), (r - 1, c - 1), (r - 1, c), (r + 1, c), (r + 1, c + 1)] {
            if let Some(id2) = get(rr, cc) { out.push(id2); }
        }
        out
    }
    /// side 0: left edge (c==0); side 1: right edge (c==r); side 2: bottom (r==n-1)
    fn on_side(&self, id: usize, side: usize) -> bool {
        let (r, c) = self.coords[id];
        match side {
            0 => c == 0,
            1 => c == r,
            _ => r == self.n - 1,
        }
    }
}

fn y_winner(tri: &Tri, occ: &[u8]) -> u8 {
    // returns 1 (player1), 2 (player2), or 0 (nobody — should be impossible on full board)
    for pl in [1u8, 2u8] {
        let mut visited = vec![false; tri.cells()];
        for start in 0..tri.cells() {
            if occ[start] != pl || visited[start] { continue; }
            let mut q = VecDeque::new();
            q.push_back(start);
            visited[start] = true;
            let mut members = vec![start];
            while let Some(cur) = q.pop_front() {
                for nb in tri.neighbors(cur) {
                    if !visited[nb] && occ[nb] == pl {
                        visited[nb] = true;
                        q.push_back(nb);
                        members.push(nb);
                    }
                }
            }
            let mut touch = [false; 3];
            for &m in &members {
                for s in 0..3 { if tri.on_side(m, s) { touch[s] = true; } }
            }
            if touch.iter().all(|&t| t) { return pl; }
        }
    }
    0
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(9);
    let trials: u32 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(5000);
    let tri = Tri::new(n);
    println!("triangle side {n}: {} cells per realm, {} total (3 realms)", tri.cells(), tri.cells() * 3);

    // 1) verify Y decisiveness on one triangle
    let mut rng = 0x1357_9BDF_2468_ACE0u64;
    let (mut p1, mut p2, mut dead) = (0u32, 0, 0);
    for _ in 0..trials {
        let cells = tri.cells();
        let mut occ = vec![0u8; cells];
        let mut ids: Vec<usize> = (0..cells).collect();
        for i in (1..cells).rev() {
            let j = (xorshift(&mut rng) % (i as u64 + 1)) as usize;
            ids.swap(i, j);
        }
        for (k, &i) in ids.iter().enumerate() { occ[i] = if k % 2 == 0 { 1 } else { 2 }; }
        match y_winner(&tri, &occ) { 1 => p1 += 1, 2 => p2 += 1, _ => dead += 1 }
    }
    let t = trials as f64;
    println!("single realm Y: P1 {:.1}% | P2 {:.1}% | NOBODY {:.2}% (theorem says 0)",
        100.0 * p1 as f64 / t, 100.0 * p2 as f64 / t, 100.0 * dead as f64 / t);

    // 2) match decisiveness: 3 independent realms, majority
    let (mut m1, mut m2) = (0u32, 0);
    for _ in 0..trials {
        let mut wins = [0u32; 3];
        for realm in 0..3 {
            let cells = tri.cells();
            let mut occ = vec![0u8; cells];
            let mut ids: Vec<usize> = (0..cells).collect();
            for i in (1..cells).rev() {
                let j = (xorshift(&mut rng) % (i as u64 + 1)) as usize;
                ids.swap(i, j);
            }
            for (k, &i) in ids.iter().enumerate() { occ[i] = if k % 2 == 0 { 1 } else { 2 }; }
            wins[realm] = y_winner(&tri, &occ) as u32;
        }
        let c1 = wins.iter().filter(|&&w| w == 1).count();
        if c1 >= 2 { m1 += 1 } else { m2 += 1 }
    }
    println!("match (majority of 3 realms): P1 {:.1}% | P2 {:.1}% | draws 0.0% BY CONSTRUCTION",
        100.0 * m1 as f64 / t, 100.0 * m2 as f64 / t);
}
