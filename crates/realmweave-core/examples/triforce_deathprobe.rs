//! Merged-triangle + death rule: random self-play length, capture activity,
//! and whether the Y theorem's decisiveness survives captures (it's no
//! longer a straightforward corollary once stones can leave the board).
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
}
fn xorshift(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

fn group_alive(tri: &Tri, occ: &[u8], start: usize) -> (Vec<usize>, bool) {
    let pl = occ[start];
    let mut seen = vec![false; occ.len()];
    let mut q = VecDeque::from([start]);
    seen[start] = true;
    let mut members = vec![start];
    let mut alive = false;
    while let Some(cur) = q.pop_front() {
        for nb in tri.neighbors(cur) {
            if occ[nb] == 0 {
                alive = true;
            } else if occ[nb] == pl && !seen[nb] {
                seen[nb] = true;
                q.push_back(nb);
                members.push(nb);
            }
        }
    }
    (members, alive)
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

fn main() {
    let n = 22usize;
    let tri = Tri::new(n);
    let cells = tri.coords.len();
    let mut rng = 0xBEEF_5678u64;
    let games = 40;
    let (mut lens, mut caps_total, mut undecided) = (Vec::new(), 0u32, 0u32);
    for _ in 0..games {
        let mut occ = vec![0u8; cells];
        let mut to_move = 1u8;
        let mut moves = 0u32;
        let mut caps = 0u32;
        let mut ko: Option<usize> = None;
        let winner = loop {
            if moves > 4000 {
                break 0;
            }
            let w = y_winner(&tri, &occ);
            if w != 0 {
                break w;
            }
            // random legal placement (not suicide, not simple-ko, not own eye)
            let mut placed = false;
            for _try in 0..cells * 2 {
                let id = (xorshift(&mut rng) % cells as u64) as usize;
                if occ[id] != 0 || ko == Some(id) {
                    continue;
                }
                // own eye skip
                if tri.neighbors(id).iter().all(|&nb| occ[nb] == to_move) {
                    continue;
                }
                occ[id] = to_move;
                // captures
                let mut captured = Vec::new();
                for nb in tri.neighbors(id) {
                    if occ[nb] == 3 - to_move {
                        let (members, alive) = group_alive(&tri, &occ, nb);
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
                caps += captured.len() as u32;
                let (_, alive) = group_alive(&tri, &occ, id);
                if !alive {
                    occ[id] = 0;
                    for &m in &captured {
                        occ[m] = 3 - to_move;
                    }
                    caps -= captured.len() as u32;
                    continue;
                }
                ko = if captured.len() == 1 {
                    Some(captured[0])
                } else {
                    None
                };
                placed = true;
                break;
            }
            if !placed {
                break 0;
            } // no legal move found (rare)
            to_move = 3 - to_move;
            moves += 1;
        };
        if winner == 0 {
            undecided += 1;
        }
        lens.push(moves);
        caps_total += caps;
    }
    lens.sort_unstable();
    println!("games {games}: undecided {undecided}, median len {} (p25 {} p75 {}), mean captures/game {:.1}",
        lens[lens.len()/2], lens[lens.len()/4], lens[3*lens.len()/4], caps_total as f64 / games as f64);
}
