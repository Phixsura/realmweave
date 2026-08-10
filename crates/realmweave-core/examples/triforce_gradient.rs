//! Does the merged triangle have an opening value gradient? For each
//! candidate first move, estimate win probability via random playouts
//! (with capture rules). Report by region: heart vs realms vs edges.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::VecDeque;

struct Tri {
    n: usize,
    coords: Vec<(usize, usize)>,
    index: Vec<Vec<usize>>,
}
impl Tri {
    fn new(n: usize) -> Self {
        let (mut coords, mut index) = (Vec::new(), Vec::new());
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
        for (rr, cc) in [
            (r, c - 1),
            (r, c + 1),
            (r - 1, c - 1),
            (r - 1, c),
            (r + 1, c),
            (r + 1, c + 1),
        ] {
            if rr >= 0 && (rr as usize) < self.n && cc >= 0 && cc <= rr {
                out.push(self.index[rr as usize][cc as usize]);
            }
        }
        out
    }
    fn sides(&self, id: usize) -> u8 {
        let (r, c) = self.coords[id];
        (u8::from(c == 0)) | (u8::from(c == r) << 1) | (u8::from(r == self.n - 1) << 2)
    }
    fn region(&self, id: usize) -> usize {
        let (r, c) = self.coords[id];
        let h = self.n / 2;
        if r < h {
            return 0;
        }
        let rr = r - h;
        if c <= rr {
            return 1;
        }
        if c >= h {
            return 2;
        }
        3
    }
}
fn xorshift(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

fn y_winner(tri: &Tri, occ: &[u8]) -> u8 {
    for pl in [1u8, 2] {
        let mut seen = vec![false; occ.len()];
        for start in 0..occ.len() {
            if occ[start] != pl || seen[start] {
                continue;
            }
            let mut q = VecDeque::from([start]);
            seen[start] = true;
            let mut touch = tri.sides(start);
            while let Some(cur) = q.pop_front() {
                for nb in tri.neighbors(cur) {
                    if occ[nb] == pl && !seen[nb] {
                        seen[nb] = true;
                        touch |= tri.sides(nb);
                        q.push_back(nb);
                    }
                }
            }
            if touch == 7 {
                return pl;
            }
        }
    }
    0
}

fn playout(tri: &Tri, occ: &mut [u8], mut to_move: u8, rng: &mut u64) -> u8 {
    let cells = occ.len();
    for _ in 0..cells * 3 {
        let w = y_winner(tri, occ);
        if w != 0 {
            return w;
        }
        let mut placed = false;
        for _t in 0..24 {
            let id = (xorshift(rng) % cells as u64) as usize;
            if occ[id] != 0 {
                continue;
            }
            if tri.neighbors(id).iter().all(|&nb| occ[nb] == to_move) {
                continue;
            }
            occ[id] = to_move;
            // captures
            let mut captured = Vec::new();
            for nb in tri.neighbors(id) {
                if occ[nb] == 3 - to_move {
                    // liberty scan
                    let mut seen = vec![false; cells];
                    let mut q = VecDeque::from([nb]);
                    seen[nb] = true;
                    let mut members = vec![nb];
                    let mut alive = false;
                    while let Some(cur) = q.pop_front() {
                        for n2 in tri.neighbors(cur) {
                            if occ[n2] == 0 {
                                alive = true;
                            } else if occ[n2] == occ[nb] && !seen[n2] {
                                seen[n2] = true;
                                q.push_back(n2);
                                members.push(n2);
                            }
                        }
                    }
                    if !alive {
                        captured.extend(members);
                    }
                }
            }
            captured.sort_unstable();
            captured.dedup();
            for &m in &captured {
                occ[m] = 0;
            }
            // suicide check
            let mut seen = vec![false; cells];
            let mut q = VecDeque::from([id]);
            seen[id] = true;
            let mut alive = false;
            while let Some(cur) = q.pop_front() {
                for n2 in tri.neighbors(cur) {
                    if occ[n2] == 0 {
                        alive = true;
                    } else if occ[n2] == to_move && !seen[n2] {
                        seen[n2] = true;
                        q.push_back(n2);
                    }
                }
            }
            if !alive {
                occ[id] = 0;
                for &m in &captured {
                    occ[m] = 3 - to_move;
                }
                continue;
            }
            placed = true;
            break;
        }
        if !placed {
            return 0;
        }
        to_move = 3 - to_move;
    }
    0
}

fn main() {
    let n = 22usize;
    let tri = Tri::new(n);
    let cells = tri.coords.len();
    let mut rng = 0xACE_2468u64;
    // sample cells: one representative per (region, ring-depth) bucket
    let mut by_region: [Vec<f64>; 4] = Default::default();
    let trials = 120;
    for id in (0..cells).step_by(7) {
        let mut wins = 0u32;
        for _ in 0..trials {
            let mut occ = vec![0u8; cells];
            occ[id] = 1;
            if playout(&tri, &mut occ, 2, &mut rng) == 1 {
                wins += 1;
            }
        }
        by_region[tri.region(id)].push(wins as f64 / trials as f64);
    }
    for (k, name) in [
        "Heaven(top)",
        "Mortal(left)",
        "Underworld(right)",
        "HEART(center)",
    ]
    .iter()
    .enumerate()
    {
        let v = &by_region[k];
        if v.is_empty() {
            continue;
        }
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let max = v.iter().cloned().fold(0.0, f64::max);
        println!(
            "{name}: first-move winrate mean {:.1}% max {:.1}% (n={})",
            mean * 100.0,
            max * 100.0,
            v.len()
        );
    }
}
