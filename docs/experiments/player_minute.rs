//! Player-experience probe of the debt world: simulate a 60s window in the
//! late game and log what a player would SEE and what choices exist.
use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};

// (same physics as collision.rs conserved branch, condensed)
const DIFFUSION: f32 = 0.22;
const RIFT_EMIT: f32 = 1.2;
const CRITICAL: f32 = 9.0;
const EJECT: f32 = 0.55;
const REFRACTORY_TICKS: u32 = 14;
const CORE_LIGHT: f32 = 35.0;
const LIGHT_DECAY: f32 = 0.8;
const AURA_DECAY: f32 = 0.25;
const KILL_CAP: f32 = 1.1;
const POOL_CRIT: f32 = 30.0;
const POOL_ERUPT_FRAC: f32 = 0.8;

struct F {
    void: Vec<f32>,
    light: Vec<f32>,
    pool: Vec<f32>,
    refr: Vec<u32>,
    built: Vec<bool>,
    hp: Vec<f32>,
}

fn step(map: &SupplyMap, f: &mut F) -> (u32, Vec<usize>) {
    let n = map.node_count();
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
                f.built[*e as usize] && f.hp[*e as usize] > 0.0 && !seen[*nb as usize]
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
    let on = seen;
    for _ in 0..2 {
        let s2 = light.clone();
        for a in 0..n {
            for &(b, _) in &map.adjacency[a] {
                let b = b as usize;
                if s2[a] > s2[b] {
                    light[b] += s2[a] * AURA_DECAY / 6.0;
                }
            }
        }
    }
    let mut nv = f.void.clone();
    for a in 0..n {
        let va = f.void[a];
        if va <= 0.0 {
            continue;
        }
        let lower: Vec<(usize, f32)> = map.adjacency[a]
            .iter()
            .map(|&(b, _)| (b as usize, va - f.void[b as usize]))
            .filter(|(b, g)| *g > 0.0 && f.refr[*b] == 0)
            .collect();
        let tg: f32 = lower.iter().map(|(_, g)| g).sum();
        if tg <= 0.0 {
            continue;
        }
        let of = (va * DIFFUSION).min(tg * 0.5);
        for (b, g) in lower {
            let s3 = of * g / tg;
            nv[a] -= s3;
            nv[b] += s3;
        }
    }
    for &r in &map.rifts {
        nv[r as usize] = (nv[r as usize] + RIFT_EMIT).min(200.0);
    }
    for i in 0..n {
        if !on[i] {
            continue;
        }
        let kill = light[i].min(nv[i]).min(KILL_CAP);
        nv[i] -= kill;
        light[i] -= kill;
        f.pool[i] += kill;
    }
    let mut erupted = Vec::new();
    for i in 0..n {
        if f.pool[i] > POOL_CRIT {
            let out = f.pool[i] * POOL_ERUPT_FRAC;
            f.pool[i] -= out;
            nv[i] += out;
            erupted.push(i);
        }
    }
    let mut collapses = 0;
    let snap = nv.clone();
    for a in 0..n {
        if snap[a] > CRITICAL && f.refr[a] == 0 {
            let mass = snap[a];
            nv[a] = 0.0;
            f.refr[a] = REFRACTORY_TICKS;
            collapses += 1;
            let nbs: Vec<usize> = map.adjacency[a]
                .iter()
                .map(|&(b, _)| b as usize)
                .filter(|&b| f.refr[b] == 0)
                .collect();
            if !nbs.is_empty() {
                let s4 = mass * EJECT / nbs.len() as f32;
                for b in nbs {
                    nv[b] += s4;
                }
            }
        }
    }
    for r in f.refr.iter_mut() {
        *r = r.saturating_sub(1);
    }
    for e in 0..map.edges.len() {
        if !f.built[e] || f.hp[e] <= 0.0 {
            continue;
        }
        let (a, b) = map.edge_endpoints(e as u32);
        let v = nv[a as usize].max(nv[b as usize]);
        let l = light[a as usize].min(light[b as usize]);
        if v > 1.5 {
            f.hp[e] -= 0.04 * (v - 1.5);
        } else if l > v && f.hp[e] < 1.0 {
            f.hp[e] = (f.hp[e] + 0.01).min(1.0);
        }
    }
    f.void = nv;
    f.light = light;
    (collapses, erupted)
}

fn build_path(map: &SupplyMap, f: &mut F, to: u16) {
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

fn main() {
    let map = generate_map(20260809, &MapSpec::default());
    let n = map.node_count();
    let mut f = F {
        void: vec![0.0; n],
        light: vec![0.0; n],
        pool: vec![0.0; n],
        refr: vec![0; n],
        built: vec![false; map.edges.len()],
        hp: vec![1.0; map.edges.len()],
    };
    let dist = map.distances(map.core);
    let mut wells: Vec<_> = map.wells.clone();
    wells.sort_by_key(|&w| dist[w as usize].unwrap_or(999));
    for &w in wells.iter().take(5) {
        build_path(&map, &mut f, w);
    }
    // run to late game
    for _ in 0..6000 {
        step(&map, &mut f);
    }
    // now: the player's 60-second window, tick by tick summary every 5s
    println!("晚期玩家的 60 秒（每 5s 一帧）:");
    println!("t | 喷发 | 坍缩 | 最大池(位置) | 池>20 的节点数 | 网络完好率");
    for w in 0..12 {
        let mut erupts = 0;
        let mut collapses = 0;
        for _ in 0..50 {
            let (c, e) = step(&map, &mut f);
            collapses += c;
            erupts += e.len();
        }
        let (max_pool_i, max_pool) = f
            .pool
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, v)| (i, *v))
            .unwrap();
        let hot = f.pool.iter().filter(|p| **p > 20.0).count();
        let alive = f
            .built
            .iter()
            .zip(f.hp.iter())
            .filter(|(b, h)| **b && **h > 0.0)
            .count();
        let total = f.built.iter().filter(|b| **b).count();
        println!(
            "{:>2} | {:>4} | {:>4} | {:>5.1}(节点{}) | {:>6} | {}/{}",
            w * 5,
            erupts,
            collapses,
            max_pool,
            max_pool_i,
            hot,
            alive,
            total
        );
    }
    println!("\n玩家可做的决策空间检查：");
    println!("- 池只增不减（无玩家动词能清空池）→ 喷发不可阻止，只能眼睁睁看");
    println!("- 池的位置由歼灭位置决定=网络形状的函数 → 改变未来的债要重构网络（慢、贵）");
    println!("- 60 秒内可操作的事：修被蚀断的线 / 加固 / 引爆(对池无效!)");
}
