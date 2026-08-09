//! COLLISION TEST: Blue's prediction vs Red's prediction.
//!
//! Blue: change annihilation from "delete void" to "store into a potential
//! pool that re-erupts past a threshold" (conservation + re-release) →
//! the system can never die; permanent intermittent dynamics.
//! Red: it will either find a new fixed point (pool cycles into a limit
//! that balances) or degenerate into unreadable noise.
//!
//! Falsifiable metrics over a LONG run (equivalent to 15 game minutes),
//! with the SAME autopilot as production:
//!   M1 late-game activity: avalanches in the final third (dead system: 0)
//!   M2 field change rate in the final third (dead: ~0)
//!   M3 intermittency: coefficient of variation of inter-avalanche gaps
//!      (regular oscillator: CV≈0 — that would be Red's "new equilibrium
//!      with extra steps"; noise: CV huge with tiny sizes; SOC-like: CV≥1
//!      with size spread ≥1 decade)
//!   M4 attributability probe: dam one rift mouth mid-run — do avalanche
//!      statistics change measurably afterward? (if not: player irrelevant
//!      → Red wins on the noise branch)

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

// Blue's rule: annihilated void goes into a per-node potential pool.
// When the pool exceeds POOL_CRIT, it erupts back as void (delayed debt).
const POOL_CRIT: f32 = 30.0;
const POOL_ERUPT_FRAC: f32 = 0.8; // fraction released on eruption

struct Field {
    void: Vec<f32>,
    light: Vec<f32>,
    pool: Vec<f32>,
    refractory: Vec<u32>,
    built: Vec<bool>,
    link_hp: Vec<f32>,
}

fn step(map: &SupplyMap, f: &mut Field, conserve: bool) -> u32 {
    let n = map.node_count();
    // light
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
    let on_network = seen;
    for _ in 0..2 {
        let snap = light.clone();
        for a in 0..n {
            for &(b, _) in &map.adjacency[a] {
                let b = b as usize;
                if snap[a] > snap[b] {
                    light[b] += snap[a] * AURA_DECAY / 6.0;
                }
            }
        }
    }
    // diffusion
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
    // annihilation: DELETE (baseline) vs POOL (blue's rule)
    for i in 0..n {
        if !on_network[i] {
            continue;
        }
        let kill = light[i].min(nv[i]).min(KILL_CAP);
        nv[i] -= kill;
        light[i] -= kill;
        if conserve {
            f.pool[i] += kill; // debt, not deletion
        }
    }
    // pool eruption (blue's rule)
    if conserve {
        for i in 0..n {
            if f.pool[i] > POOL_CRIT {
                let out = f.pool[i] * POOL_ERUPT_FRAC;
                f.pool[i] -= out;
                nv[i] += out;
            }
        }
    }
    // SOC collapse
    let mut collapses = 0;
    let snap = nv.clone();
    for a in 0..n {
        if snap[a] > CRITICAL && f.refractory[a] == 0 {
            let mass = snap[a];
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
    // erosion
    for e in 0..map.edges.len() {
        if !f.built[e] || f.link_hp[e] <= 0.0 {
            continue;
        }
        let (a, b) = map.edge_endpoints(e as u32);
        let v = nv[a as usize].max(nv[b as usize]);
        let l = light[a as usize].min(light[b as usize]);
        if v > 1.5 {
            f.link_hp[e] -= 0.04 * (v - 1.5);
        } else if l > v && f.link_hp[e] < 1.0 {
            f.link_hp[e] = (f.link_hp[e] + 0.01).min(1.0);
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

struct Verdict {
    late_avalanches: usize,
    late_change_rate: f32,
    gap_cv: f32,
    size_spread: f32,
    dam_effect: f32,
}

fn analyze(map: &SupplyMap, conserve: bool, dam_mid_run: bool) -> Verdict {
    let n = map.node_count();
    let mut f = Field {
        void: vec![0.0; n],
        light: vec![0.0; n],
        pool: vec![0.0; n],
        refractory: vec![0; n],
        built: vec![false; map.edges.len()],
        link_hp: vec![1.0; map.edges.len()],
    };
    // autopilot-equivalent static network: 5 nearest wells
    let dist = map.distances(map.core);
    let mut wells: Vec<_> = map.wells.clone();
    wells.sort_by_key(|&w| dist[w as usize].unwrap_or(999));
    for &w in wells.iter().take(5) {
        build_path(map, &mut f, w);
    }

    const TOTAL: u32 = 9000;
    let mut avalanche_ticks: Vec<(u32, u32)> = Vec::new(); // (tick, size)
    let mut run = 0u32;
    let mut pre_dam = 0usize;
    let mut post_dam = 0usize;
    let mut prev_void = f.void.clone();
    let mut late_change = 0.0f32;
    let mut late_samples = 0;
    for t in 0..TOTAL {
        if dam_mid_run && t == TOTAL / 2 {
            // player action mid-run: dam EVERY rift mouth (max intervention)
            for &rift in &map.rifts {
                let dr = map.distances(rift);
                let dc = map.distances(map.core);
                if let Some(head) = (0..n as u16)
                    .filter(|&nd| dr[nd as usize] == Some(2))
                    .min_by_key(|&nd| dc[nd as usize].unwrap_or(999))
                {
                    build_path(map, &mut f, head);
                }
            }
        }
        let c = step(map, &mut f, conserve);
        if c > 0 {
            run += c;
        } else if run > 0 {
            avalanche_ticks.push((t, run));
            if t < TOTAL / 2 {
                pre_dam += 1;
            } else {
                post_dam += 1;
            }
            run = 0;
        }
        if t >= TOTAL * 2 / 3 && t % 100 == 0 {
            let d: f32 = f
                .void
                .iter()
                .zip(prev_void.iter())
                .map(|(a, b)| (a - b).abs())
                .sum();
            late_change += d;
            late_samples += 1;
            prev_void = f.void.clone();
        }
    }
    let late: Vec<&(u32, u32)> = avalanche_ticks
        .iter()
        .filter(|(t, _)| *t >= TOTAL * 2 / 3)
        .collect();
    // inter-avalanche gap CV
    let gaps: Vec<f32> = avalanche_ticks
        .windows(2)
        .map(|w| (w[1].0 - w[0].0) as f32)
        .collect();
    let gap_cv = if gaps.len() > 3 {
        let mean = gaps.iter().sum::<f32>() / gaps.len() as f32;
        let var = gaps.iter().map(|g| (g - mean).powi(2)).sum::<f32>() / gaps.len() as f32;
        var.sqrt() / mean.max(0.001)
    } else {
        -1.0
    };
    let sizes: Vec<u32> = avalanche_ticks.iter().map(|(_, s)| *s).collect();
    let size_spread = if sizes.is_empty() {
        0.0
    } else {
        *sizes.iter().max().unwrap() as f32 / (*sizes.iter().min().unwrap() as f32).max(1.0)
    };
    let dam_effect = if pre_dam > 0 {
        (pre_dam as f32 - post_dam as f32) / pre_dam as f32
    } else {
        0.0
    };
    Verdict {
        late_avalanches: late.len(),
        late_change_rate: late_change / late_samples.max(1) as f32,
        gap_cv,
        size_spread,
        dam_effect,
    }
}

fn locate_avalanches(map: &SupplyMap) {
    // Where do late-game collapses happen in the conserved world?
    let n = map.node_count();
    let mut f = Field {
        void: vec![0.0; n],
        light: vec![0.0; n],
        pool: vec![0.0; n],
        refractory: vec![0; n],
        built: vec![false; map.edges.len()],
        link_hp: vec![1.0; map.edges.len()],
    };
    let dist = map.distances(map.core);
    let mut wells: Vec<_> = map.wells.clone();
    wells.sort_by_key(|&w| dist[w as usize].unwrap_or(999));
    for &w in wells.iter().take(5) {
        build_path(map, &mut f, w);
    }
    let mut on_net_collapses = 0u32;
    let mut off_net_collapses = 0u32;
    for t in 0..9000u32 {
        // track which nodes are on the light network
        let mut on = vec![false; n];
        on[map.core as usize] = true;
        let mut stack = vec![map.core];
        while let Some(cur) = stack.pop() {
            for &(nb, e) in &map.adjacency[cur as usize] {
                if f.built[e as usize] && f.link_hp[e as usize] > 0.0 && !on[nb as usize] {
                    on[nb as usize] = true;
                    stack.push(nb);
                }
            }
        }
        let before: Vec<u32> = f.refractory.clone();
        step(map, &mut f, true);
        if t >= 6000 {
            for i in 0..n {
                if f.refractory[i] == REFRACTORY_TICKS - 1 && before[i] == 0 {
                    // fresh collapse this tick... approximately: refractory
                    // just set. Distance to nearest network node:
                    if on[i] {
                        on_net_collapses += 1;
                    } else {
                        off_net_collapses += 1;
                    }
                }
            }
        }
    }
    println!(
        "\n末段雪崩位置分析（守恒版）: 网络上 {on_net_collapses} vs 网络外 {off_net_collapses}"
    );
    println!(
        "→ 若大半在网络上：玩家的防线本身成了火山（红方噪音支部分成立，但也是绝妙的游戏机制）"
    );
}

fn main() {
    let map = generate_map(20260809, &MapSpec::default());
    println!("15 分钟等效长跑（9000 tick），同一自动驾驶网络：\n");
    println!(
        "{:<18} | 末段雪崩 | 末段场变化率 | 间隔CV | 规模跨度 | 筑坝效应",
        "版本"
    );
    for (name, conserve) in [("基线(删除式歼灭)", false), ("蓝方(守恒+势能池)", true)]
    {
        let v = analyze(&map, conserve, true);
        println!(
            "{:<18} | {:>8} | {:>12.1} | {:>6.2} | {:>8.1}x | {:>+7.0}%",
            name,
            v.late_avalanches,
            v.late_change_rate,
            v.gap_cv,
            v.size_spread,
            v.dam_effect * 100.0
        );
    }
    locate_avalanches(&map);
    println!("\n判决标准：");
    println!("  蓝方胜 = 守恒版末段雪崩>0 且 场变化率>1 且 CV≥0.8 且 跨度≥10x 且 筑坝效应显著");
    println!("  红方胜(新稳态支) = 末段雪崩≈0 或 CV<0.3(规则振荡=换皮稳态)");
    println!("  红方胜(噪音支)   = 雪崩极频繁但规模跨度<3x 且 筑坝效应≈0(玩家无关)");
}
