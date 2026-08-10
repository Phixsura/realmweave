//! Probe the "Triforce" geometry: ONE big triangle (side 2N) whose three
//! corner sub-triangles are the realms and the central inverted triangle is
//! the weave-heart. Y goal on the big triangle's three sides.
//! Q1: decisiveness on random fill (Y theorem should still hold: 100%).
//! Q2: do winning groups cross realm boundaries (coupling made real)?
//! Q3: how often does the winning group use the heart?
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::VecDeque;

struct Tri {
    n: usize,
    coords: Vec<(usize, usize)>,
    index: Vec<Vec<usize>>,
}
impl Tri {
    fn new(n: usize) -> Self {
        let mut coords = Vec::new();
        let mut index = Vec::new();
        for r in 0..n {
            let mut row = Vec::new();
            for c in 0..=r {
                row.push(coords.len());
                coords.push((r, c));
            }
            index.push(row);
        }
        Tri { n, coords, index }
    }
    fn neighbors(&self, id: usize) -> Vec<usize> {
        let (r, c) = self.coords[id];
        let (r, c) = (r as i64, c as i64);
        let mut out = Vec::new();
        for (rr, cc) in [(r, c - 1), (r, c + 1), (r - 1, c - 1), (r - 1, c), (r + 1, c), (r + 1, c + 1)] {
            if rr >= 0 && (rr as usize) < self.n && cc >= 0 && cc <= rr {
                out.push(self.index[rr as usize][cc as usize]);
            }
        }
        out
    }
    fn sides(&self, id: usize) -> u8 {
        let (r, c) = self.coords[id];
        let mut m = 0;
        if c == 0 { m |= 1 }
        if c == r { m |= 2 }
        if r == self.n - 1 { m |= 4 }
        m
    }
    /// Region: 0/1/2 = corner realms (top/left/right), 3 = central heart.
    fn region(&self, id: usize) -> usize {
        let (r, c) = self.coords[id];
        let h = self.n / 2;
        if r < h { return 0; } // top realm (Heaven)
        // bottom half: left realm if c < r-h+? Use sub-triangle test:
        // left corner sub-tri: rows h..n, cols 0..(r-h)
        if c <= r - h { /* wait: left subtriangle is c in 0..=(r-h)? */ }
        let rr = r - h;
        if c <= rr { return 1; } // left (Mortal)
        if c >= h { return 2; } // right (Underworld) — cols h..=r
        3 // heart
    }
}

fn xorshift(x: &mut u64) -> u64 { *x ^= *x << 13; *x ^= *x >> 7; *x ^= *x << 17; *x }

fn main() {
    let n = 22usize; // big triangle side (2*11)
    let tri = Tri::new(n);
    let cells = tri.coords.len();
    println!("triforce side {n}: {cells} cells; regions sized {:?}",
        (0..4).map(|k| (0..cells).filter(|&i| tri.region(i) == k).count()).collect::<Vec<_>>());
    let trials = 3000;
    let (mut p1, mut p2, mut dead) = (0u32, 0u32, 0u32);
    let mut crossers = 0u32;
    let mut heart_users = 0u32;
    let mut rng = 0xF00D_1234u64;
    for _ in 0..trials {
        let mut occ = vec![0u8; cells];
        let mut ids: Vec<usize> = (0..cells).collect();
        for i in (1..cells).rev() {
            let j = (xorshift(&mut rng) % (i as u64 + 1)) as usize;
            ids.swap(i, j);
        }
        for (k, &i) in ids.iter().enumerate() { occ[i] = if k % 2 == 0 { 1 } else { 2 }; }
        // find Y winner + inspect the winning group
        let mut winner = 0u8;
        'outer: for pl in [1u8, 2] {
            let mut seen = vec![false; cells];
            for start in 0..cells {
                if occ[start] != pl || seen[start] { continue; }
                let mut q = VecDeque::from([start]);
                seen[start] = true;
                let mut members = vec![start];
                while let Some(cur) = q.pop_front() {
                    for nb in tri.neighbors(cur) {
                        if !seen[nb] && occ[nb] == pl {
                            seen[nb] = true; q.push_back(nb); members.push(nb);
                        }
                    }
                }
                let mut touch = 0u8;
                for &m in &members { touch |= tri.sides(m); }
                if touch == 7 {
                    winner = pl;
                    let regions: std::collections::HashSet<usize> =
                        members.iter().map(|&m| tri.region(m)).collect();
                    if regions.iter().filter(|&&x| x < 3).count() >= 2 { crossers += 1; }
                    if regions.contains(&3) { heart_users += 1; }
                    break 'outer;
                }
            }
        }
        match winner { 1 => p1 += 1, 2 => p2 += 1, _ => dead += 1 }
    }
    let t = trials as f64;
    println!("decisive: {:.2}% (P1 {:.1}% / P2 {:.1}%), dead {:.2}%",
        100.0 * (p1 + p2) as f64 / t, 100.0 * p1 as f64 / t, 100.0 * p2 as f64 / t, 100.0 * dead as f64 / t);
    println!("winning group crosses >=2 realms: {:.1}%", 100.0 * crossers as f64 / (p1 + p2) as f64);
    println!("winning group uses the heart:     {:.1}%", 100.0 * heart_users as f64 / (p1 + p2) as f64);
}
