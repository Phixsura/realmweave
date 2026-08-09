//! Emergence v3: the SOC void field MEETS the light network.
//!
//! Field rules (from v2, SOC-verified):
//!   - flow-limited diffusion; rifts emit constantly
//!   - overcrowding collapse: void > CRITICAL → zero + shockwave + refractory
//! Light rules (from v1, drama-zone tuned):
//!   - core light flows along built links, decaying per hop
//!   - network nodes annihilate void with finite throughput (KILL_CAP)
//!   - links erode where void is high, self-repair where light dominates
//!
//! THE QUESTION: does the player have leverage over the earthquakes?
//! Tests:
//!   T1 wall-shape: same map, three build strategies → do avalanche
//!      statistics and survival differ significantly?
//!   T2 pressure-cooker: does a wall CLOSE to a rift cause bigger, rarer
//!      avalanches than a wall FAR from it (dam = bigger quakes)?
//!   T3 keyframes for the eyeball test.

use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};

const DIFFUSION: f32 = 0.22;
const RIFT_EMIT: f32 = 1.2;
const CRITICAL: f32 = 9.0;
const EJECT: f32 = 0.55;
const REFRACTORY_TICKS: u32 = 14;

const CORE_LIGHT: f32 = 35.0;
const LIGHT_DECAY: f32 = 0.8;
const AURA_DECAY: f32 = 0.25;
const KILL_CAP: f32 = 1.1;
const ERODE_THRESHOLD: f32 = 1.5;
const ERODE_RATE: f32 = 0.04;
const REPAIR_RATE: f32 = 0.01;

#[derive(Clone)]
struct Field {
    void: Vec<f32>,
    light: Vec<f32>,
    refractory: Vec<u32>,
    link_hp: Vec<f32>,
    built: Vec<bool>,
}

impl Field {
    fn new(map: &SupplyMap) -> Field {
        Field {
            void: vec![0.0; map.node_count()],
            light: vec![0.0; map.node_count()],
            refractory: vec![0; map.node_count()],
            link_hp: vec![1.0; map.edges.len()],
            built: vec![false; map.edges.len()],
        }
    }
}

/// One tick. Returns number of collapses this tick (avalanche tracking).
fn step(map: &SupplyMap, f: &mut Field) -> u32 {
    let n = map.node_count();

    // --- light: BFS flow along intact built links ---
    let mut light = vec![0.0f32; n];
    light[map.core as usize] = CORE_LIGHT;
    let mut order = vec![map.core];
    let mut seen = vec![false; n];
    seen[map.core as usize] = true;
    let mut i = 0;
    while i < order.len() {
        let cur = order[i];
        i += 1;
        let outs: Vec<(u16, u32)> = map.adjacency[cur as usize]
            .iter()
            .copied()
            .filter(|(nb, e)| {
                f.built[*e as usize] && f.link_hp[*e as usize] > 0.0 && !seen[*nb as usize]
            })
            .collect();
        if outs.is_empty() {
            continue;
        }
        let share = light[cur as usize] * LIGHT_DECAY / outs.len() as f32;
        for (nb, _) in outs {
            light[nb as usize] += share;
            seen[nb as usize] = true;
            order.push(nb);
        }
    }
    let on_network = seen.clone();

    // aura (soft light: protects, doesn't kill)
    for _ in 0..2 {
        let snapshot = light.clone();
        for a in 0..n {
            for &(b, _) in &map.adjacency[a] {
                let b = b as usize;
                if snapshot[a] > snapshot[b] {
                    light[b] += snapshot[a] * AURA_DECAY / 6.0;
                }
            }
        }
    }

    // --- void diffusion (flow-limited; refractory nodes accept nothing) ---
    let mut nv = f.void.clone();
    for a in 0..n {
        let va = f.void[a];
        if va <= 0.0 {
            continue;
        }
        let lower: Vec<(usize, f32)> = map.adjacency[a]
            .iter()
            .map(|&(b, _)| (b as usize, va - f.void[b as usize]))
            .filter(|(b, g)| *g > 0.0 && f.refractory[*b] == 0)
            .collect();
        let tg: f32 = lower.iter().map(|(_, g)| g).sum();
        if tg <= 0.0 {
            continue;
        }
        let outflow = (va * DIFFUSION).min(tg * 0.5);
        for (b, g) in lower {
            let share = outflow * g / tg;
            nv[a] -= share;
            nv[b] += share;
        }
    }
    for &r in &map.rifts {
        nv[r as usize] = (nv[r as usize] + RIFT_EMIT).min(200.0);
    }

    // --- annihilation: hard light only, finite throughput ---
    for i in 0..n {
        if !on_network[i] {
            continue;
        }
        let kill = light[i].min(nv[i]).min(KILL_CAP);
        nv[i] = (nv[i] - kill).max(0.0);
        light[i] = (light[i] - kill).max(0.0);
    }

    // --- collapse (SOC rule) ---
    let mut collapses = 0;
    let snapshot = nv.clone();
    for a in 0..n {
        if snapshot[a] > CRITICAL && f.refractory[a] == 0 {
            let mass = snapshot[a];
            nv[a] = 0.0;
            f.refractory[a] = REFRACTORY_TICKS;
            collapses += 1;
            let nbs: Vec<usize> = map.adjacency[a]
                .iter()
                .map(|&(b, _)| b as usize)
                .filter(|&b| f.refractory[b] == 0)
                .collect();
            if !nbs.is_empty() {
                let share = mass * EJECT / nbs.len() as f32;
                for b in nbs {
                    nv[b] += share;
                }
            }
        }
    }
    for r in f.refractory.iter_mut() {
        *r = r.saturating_sub(1);
    }

    // --- erosion / repair ---
    for e in 0..map.edges.len() {
        if !f.built[e] {
            continue;
        }
        let (a, b) = map.edge_endpoints(e as u32);
        let (a, b) = (a as usize, b as usize);
        let v = nv[a].max(nv[b]);
        let l = light[a].min(light[b]);
        if v > ERODE_THRESHOLD && f.link_hp[e] > 0.0 {
            f.link_hp[e] -= ERODE_RATE * (v - ERODE_THRESHOLD);
        } else if l > v && f.link_hp[e] < 1.0 {
            f.link_hp[e] = (f.link_hp[e] + REPAIR_RATE).min(1.0);
        }
    }

    f.void = nv;
    f.light = light;
    collapses
}

fn build_path(map: &SupplyMap, f: &mut Field, to: u16) {
    let dist = map.distances(map.core);
    let mut cur = to;
    while cur != map.core {
        let d = match dist[cur as usize] {
            Some(d) => d,
            None => return,
        };
        if let Some(&(prev, edge)) = map.adjacency[cur as usize]
            .iter()
            .find(|(nb, _)| dist[*nb as usize] == Some(d - 1))
        {
            f.built[edge as usize] = true;
            cur = prev;
        } else {
            return;
        }
    }
}

fn render(map: &SupplyMap, f: &Field, label: &str) -> String {
    let mut out = format!("--- {label} ---\n");
    let min_r = map.axial.iter().map(|a| a[1]).min().unwrap();
    let max_r = map.axial.iter().map(|a| a[1]).max().unwrap();
    let min_q = map.axial.iter().map(|a| a[0]).min().unwrap();
    let max_q = map.axial.iter().map(|a| a[0]).max().unwrap();
    for r in min_r..=max_r {
        out.push_str(&" ".repeat((r - min_r) as usize));
        for q in min_q..=max_q {
            let node = map.axial.iter().position(|a| *a == [q, r]);
            let ch = match node {
                None => ' ',
                Some(i) => {
                    let i16 = i as u16;
                    if i16 == map.core {
                        '@'
                    } else if map.rifts.contains(&i16) {
                        'R'
                    } else if f.light[i] > 1.0 {
                        '*'
                    } else if f.refractory[i] > 0 {
                        'o'
                    } else if f.void[i] > 7.0 {
                        '#'
                    } else if f.void[i] > 3.5 {
                        '+'
                    } else if f.void[i] > 1.0 {
                        '.'
                    } else {
                        '·'
                    }
                }
            };
            out.push(ch);
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

struct RunStats {
    avalanches: Vec<u32>,
    lit_final: usize,
    broken: usize,
    biggest: u32,
}

fn run(map: &SupplyMap, f: &mut Field, ticks: u32, frames_at: &[u32]) -> RunStats {
    let mut avalanches = Vec::new();
    let mut current = 0u32;
    for t in 0..ticks {
        let c = step(map, f);
        if c > 0 {
            current += c;
        } else if current > 0 {
            avalanches.push(current);
            current = 0;
        }
        if frames_at.contains(&t) {
            println!("{}", render(map, f, &format!("t={t}")));
        }
    }
    let lit_final = f.light.iter().filter(|l| **l > 1.0).count();
    let broken = f
        .built
        .iter()
        .zip(f.link_hp.iter())
        .filter(|(b, hp)| **b && **hp <= 0.0)
        .count();
    let biggest = avalanches.iter().max().copied().unwrap_or(0);
    RunStats {
        avalanches,
        lit_final,
        broken,
        biggest,
    }
}

fn summarize(name: &str, s: &RunStats) {
    let n = s.avalanches.len().max(1);
    let mean: f32 = s.avalanches.iter().sum::<u32>() as f32 / n as f32;
    println!(
        "{name}: 雪崩 {} 次 | 均值 {mean:.1} | 最大 {} | 终局亮区 {} | 蚀断 {}",
        s.avalanches.len(),
        s.biggest,
        s.lit_final,
        s.broken
    );
}

fn main() {
    let map = generate_map(20260809, &MapSpec::default());
    let dist = map.distances(map.core);

    // ============ T1: three strategies, same world ============
    println!("========== T1：三种建网策略 vs 同一个 SOC 场 ==========\n");

    // Strategy 1: no walls at all (baseline).
    let mut f_none = Field::new(&map);
    let s_none = run(&map, &mut f_none, 8000, &[]);

    // Strategy 2: greedy sprawl — light highways to the 4 nearest wells.
    let mut f_sprawl = Field::new(&map);
    {
        let mut wells: Vec<_> = map.wells.clone();
        wells.sort_by_key(|&w| dist[w as usize].unwrap_or(999));
        for &w in wells.iter().take(4) {
            build_path(&map, &mut f_sprawl, w);
        }
    }
    println!(">> 策略「铺开」初始形态与中期：");
    let s_sprawl = run(&map, &mut f_sprawl, 8000, &[0, 4000]);

    // Strategy 3: fortress — short dense ring near the core (all edges
    // within distance 2).
    let mut f_fort = Field::new(&map);
    for e in 0..map.edges.len() as u32 {
        let (a, b) = map.edge_endpoints(e);
        if dist[a as usize].unwrap_or(99) <= 2 && dist[b as usize].unwrap_or(99) <= 2 {
            f_fort.built[e as usize] = true;
        }
    }
    println!(">> 策略「堡垒」中期：");
    let s_fort = run(&map, &mut f_fort, 8000, &[4000]);

    println!("\n对比：");
    summarize("  无墙   ", &s_none);
    summarize("  铺开   ", &s_sprawl);
    summarize("  堡垒   ", &s_fort);

    // ============ T2: dam distance vs avalanche size ============
    println!("\n========== T2：坝的位置改变地震的形状吗？ ==========");
    let rift = map.rifts[0];
    let dist_r = map.distances(rift);

    // near-dam: wall at distance 2 from the rift
    let mut f_near = Field::new(&map);
    {
        let head = (0..map.node_count() as u16)
            .filter(|&nd| dist_r[nd as usize] == Some(2))
            .min_by_key(|&nd| dist[nd as usize].unwrap_or(999))
            .unwrap();
        build_path(&map, &mut f_near, head);
    }
    let s_near = run(&map, &mut f_near, 8000, &[]);

    // far-dam: wall at distance 5 from the rift
    let mut f_far = Field::new(&map);
    {
        let head = (0..map.node_count() as u16)
            .filter(|&nd| dist_r[nd as usize] == Some(5))
            .min_by_key(|&nd| dist[nd as usize].unwrap_or(999))
            .unwrap();
        build_path(&map, &mut f_far, head);
    }
    let s_far = run(&map, &mut f_far, 8000, &[]);

    summarize("  近坝（贴脸压制）", &s_near);
    summarize("  远坝（纵深防御）", &s_far);
    // Avalanche size histograms side by side
    let hist = |v: &Vec<u32>| -> Vec<(u32, u32)> {
        let mut h = std::collections::BTreeMap::new();
        for &s in v {
            let bin = 32 - s.max(1).leading_zeros();
            *h.entry(bin).or_insert(0u32) += 1;
        }
        h.into_iter().collect()
    };
    println!("  近坝雪崩分布: {:?}", hist(&s_near.avalanches));
    println!("  远坝雪崩分布: {:?}", hist(&s_far.avalanches));
}
