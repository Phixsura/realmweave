//! Emergence experiment: can THREE local rules generate the game?
//!
//! Rule 1 (void diffusion): void concentration v diffuses along edges to
//!   lower-concentration neighbors; rift nodes are constant sources.
//! Rule 2 (light suppression): the core emits light which flows along BUILT
//!   links, attenuating with distance/branching; light and void annihilate
//!   each other 1:1 each tick where they meet.
//! Rule 3 (erosion): while a node's void exceeds a threshold, links touching
//!   it erode; fully eroded links break.
//!
//! No waves, no unit AI, no schedules. We test three claims:
//!   A. stable fronts form where light meets void;
//!   B. damming void causes pressure buildup and eventual bursts;
//!   C. different topologies produce different battles.

use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};

const DIFFUSION: f32 = 0.25;
const RIFT_EMIT: f32 = 1.3;
const CORE_LIGHT: f32 = 35.0;
const LIGHT_DECAY: f32 = 0.8;
const AURA_DECAY: f32 = 0.2; // Rule 4a: light radiates off-network
const REPAIR_RATE: f32 = 0.02; // Rule 4b: light-dominated links self-heal
const ANNIHILATE: f32 = 1.0;
/// Max void a node's light can annihilate per tick (defensive throughput).
/// Finite throughput is what lets pressure ACCUMULATE against a dam:
/// inflow > kill capacity → the excess piles up → bursts become possible.
const KILL_CAP: f32 = 1.1;
const ERODE_THRESHOLD: f32 = 1.5;
const ERODE_RATE: f32 = 0.04;
// Conservation experiment: NO cap — void total changes only via rift
// inflow (constant) and light annihilation (the only sink). Dammed void
//必然 accumulates; bursts become mathematically inevitable when inflow
// exceeds the front's kill rate.

#[derive(Clone)]
struct Field {
    void: Vec<f32>,
    light: Vec<f32>,
    /// 1.0 = intact, 0.0 = broken. Only BUILT links participate.
    link_hp: Vec<f32>,
    built: Vec<bool>,
}

impl Field {
    fn new(map: &SupplyMap) -> Field {
        Field {
            void: vec![0.0; map.node_count()],
            light: vec![0.0; map.node_count()],
            link_hp: vec![1.0; map.edges.len()],
            built: vec![false; map.edges.len()],
        }
    }
}

fn step(map: &SupplyMap, f: &mut Field) {
    let n = map.node_count();

    // --- Rule 2a: light propagation (fresh each tick: BFS flow from core
    // along built+intact links with per-hop decay; branches split flow).
    let mut light = vec![0.0f32; n];
    light[map.core as usize] = CORE_LIGHT;
    // BFS by distance over built links.
    let mut order = vec![map.core];
    let mut seen = vec![false; n];
    seen[map.core as usize] = true;
    let mut i = 0;
    while i < order.len() {
        let cur = order[i];
        i += 1;
        // outgoing built links
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
            if !seen[nb as usize] {
                seen[nb as usize] = true;
                order.push(nb);
            }
        }
    }

    let on_network = seen.clone();

    // --- Rule 4a: aura — light radiates beyond the built network.
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

    // --- Rule 1: void diffusion — flow-limited (numerically stable):
    // each node distributes at most DIFFUSION of its content per tick,
    // split among strictly-lower neighbors proportionally to the gradient.
    let mut new_void = f.void.clone();
    for a in 0..n {
        let va = f.void[a];
        if va <= 0.0 {
            continue;
        }
        let lower: Vec<(usize, f32)> = map.adjacency[a]
            .iter()
            .map(|&(b, _)| (b as usize, va - f.void[b as usize]))
            .filter(|(_, g)| *g > 0.0)
            .collect();
        let total_gradient: f32 = lower.iter().map(|(_, g)| g).sum();
        if total_gradient <= 0.0 {
            continue;
        }
        let outflow = (va * DIFFUSION).min(total_gradient * 0.5);
        for (b, g) in lower {
            let share = outflow * g / total_gradient;
            new_void[a] -= share;
            new_void[b] += share;
        }
    }
    for &r in &map.rifts {
        // Constant inflow with a generous local ceiling (pooling preserved,
        // explosion prevented).
        new_void[r as usize] = (new_void[r as usize] + RIFT_EMIT).min(200.0);
    }

    // --- Rule 2b: annihilation — ONLY on the network itself (network
    // light is "hard" light; aura is soft — it protects but cannot kill).
    // Finite throughput per node (KILL_CAP).
    for i in 0..n {
        if !on_network[i] {
            continue;
        }
        let kill = light[i].min(new_void[i] * ANNIHILATE).min(KILL_CAP);
        new_void[i] = (new_void[i] - kill).max(0.0);
        light[i] = (light[i] - kill / ANNIHILATE).max(0.0);
    }

    // --- Rule 3: erosion + Rule 4b: self-repair when light-dominated.
    for e in 0..map.edges.len() {
        if !f.built[e] {
            continue;
        }
        let (a, b) = map.edge_endpoints(e as u32);
        let v = new_void[a as usize].max(new_void[b as usize]);
        let l = light[a as usize].min(light[b as usize]);
        if v > ERODE_THRESHOLD && f.link_hp[e] > 0.0 {
            f.link_hp[e] -= ERODE_RATE * (v - ERODE_THRESHOLD);
        } else if l > v && f.link_hp[e] < 1.0 {
            f.link_hp[e] = (f.link_hp[e] + REPAIR_RATE).min(1.0);
        }
    }

    f.void = new_void;
    f.light = light;
}

/// ASCII snapshot: rows of the hex disc; each node one glyph.
fn render(map: &SupplyMap, f: &Field, label: &str) -> String {
    let mut out = format!("--- {label} ---\n");
    let min_r = map.axial.iter().map(|a| a[1]).min().unwrap();
    let max_r = map.axial.iter().map(|a| a[1]).max().unwrap();
    let min_q = map.axial.iter().map(|a| a[0]).min().unwrap();
    let max_q = map.axial.iter().map(|a| a[0]).max().unwrap();
    for r in min_r..=max_r {
        let indent = (r - min_r) as usize;
        out.push_str(&" ".repeat(indent));
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
                        '*' // lit territory
                    } else if f.void[i] > 8.0 {
                        '#' // deep void
                    } else if f.void[i] > ERODE_THRESHOLD {
                        '+' // dangerous void
                    } else if f.void[i] > 0.3 {
                        '.' // void mist
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

/// Build a light-highway from core toward a target (straight BFS path).
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

fn main() {
    // ------------------------------------------------ Experiment A: fronts
    println!("############ 试验 A：光-虚空前线会稳定成形吗？ ############");
    let map = generate_map(20260809, &MapSpec::default());
    let mut f = Field::new(&map);
    // Build a modest network toward the middle of the map.
    let dist = map.distances(map.core);
    let mid_targets: Vec<u16> = (0..map.node_count() as u16)
        .filter(|&nd| dist[nd as usize] == Some(4))
        .take(3)
        .collect();
    for &t in &mid_targets {
        build_path(&map, &mut f, t);
    }
    for t in 0..=1200 {
        step(&map, &mut f);
        if t % 400 == 0 {
            println!("{}", render(&map, &f, &format!("t={t}")));
        }
    }
    // Front stability metric: count boundary nodes (lit next to void>thresh)
    let mut frontier = 0;
    for a in 0..map.node_count() {
        if f.light[a] > 1.0 {
            for &(b, _) in &map.adjacency[a] {
                if f.void[b as usize] > ERODE_THRESHOLD {
                    frontier += 1;
                    break;
                }
            }
        }
    }
    println!("前线宽度（亮节点邻接危险虚空的数量）: {frontier}");
    let broken = f
        .built
        .iter()
        .zip(f.link_hp.iter())
        .filter(|(b, hp)| **b && **hp <= 0.0)
        .count();
    println!("被蚀断的线路: {broken}\n");

    // -------------------------------------------- Experiment B: dam & burst
    println!("############ 试验 B：筑坝会积压、决堤吗？（修正版：坝在半路） ############");
    let mut f2 = Field::new(&map);
    // Dam MIDWAY: build the highway only up to distance 3 SHORT of the
    // rift — void pools in the unlit pocket between rift and dam head.
    let rift = map.rifts[0];
    let dist_r = map.distances(rift);
    let dist_c = map.distances(map.core);
    // dam head: on the core-rift shortest path, 3 hops from the rift
    let dam_head = (0..map.node_count() as u16)
        .filter(|&nd| dist_r[nd as usize] == Some(3))
        .min_by_key(|&nd| dist_c[nd as usize].unwrap_or(999))
        .unwrap();
    build_path(&map, &mut f2, dam_head);
    let mut pressure_history: Vec<f32> = Vec::new();
    let mut burst_tick = None;
    let mut max_pressure = 0.0f32;
    for t in 0..=6000 {
        step(&map, &mut f2);
        if t % 1500 == 0 {
            let total: f32 = f2.void.iter().sum();
            let dam_hp: f32 = f2
                .built
                .iter()
                .zip(f2.link_hp.iter())
                .filter(|(b, _)| **b)
                .map(|(_, hp)| *hp)
                .sum::<f32>()
                / f2.built.iter().filter(|b| **b).count().max(1) as f32;
            println!(
                "  t={t}: 全图虚空总量 {total:.0}, 坝区积压 {:.1}, 坝平均HP {dam_hp:.2}",
                pressure_history.last().unwrap_or(&0.0)
            );
        }
        // pressure = total void pooled between rift and the dam (dist_r<=2)
        let pressure: f32 = (0..map.node_count())
            .filter(|&i| dist_r[i].unwrap_or(99) <= 2)
            .map(|i| f2.void[i])
            .sum();
        pressure_history.push(pressure);
        max_pressure = max_pressure.max(pressure);
        if burst_tick.is_none()
            && f2
                .built
                .iter()
                .zip(f2.link_hp.iter())
                .any(|(b, hp)| *b && *hp <= 0.0)
        {
            burst_tick = Some(t);
        }
    }
    let p0 = pressure_history[100].max(0.1);
    println!(
        "t=100 积压 {p0:.1} → 峰值 {max_pressure:.1}（{:.0}x）",
        max_pressure / p0
    );
    match burst_tick {
        Some(t) => {
            println!("决堤！t={t}，坝头线路被蚀穿。决堤后虚空是否倾泻？");
            // post-burst: measure void reaching within 2 of the CORE
            let dc = map.distances(map.core);
            let near_core: f32 = (0..map.node_count())
                .filter(|&i| dc[i].unwrap_or(99) <= 2)
                .map(|i| f2.void[i])
                .sum();
            println!("终局母核周边虚空: {near_core:.1}（>5 = 倾泻真实发生）");
        }
        None => println!("6000 tick 未决堤"),
    }
    println!();

    // ----------------------------------- Experiment C: topology sensitivity
    println!("############ 试验 C：不同拓扑产生不同战局吗？ ############");
    let mut summaries = Vec::new();
    for seed in [1u64, 2, 3, 4, 5] {
        let m = generate_map(seed, &MapSpec::default());
        let mut fc = Field::new(&m);
        let d = m.distances(m.core);
        let targets: Vec<u16> = (0..m.node_count() as u16)
            .filter(|&nd| d[nd as usize] == Some(4))
            .take(3)
            .collect();
        for &t in &targets {
            build_path(&m, &mut fc, t);
        }
        for _ in 0..1500 {
            step(&m, &mut fc);
        }
        let lit = fc.light.iter().filter(|l| **l > 1.0).count();
        let deep = fc.void.iter().filter(|v| **v > 8.0).count();
        let broken = fc
            .built
            .iter()
            .zip(fc.link_hp.iter())
            .filter(|(b, hp)| **b && **hp <= 0.0)
            .count();
        summaries.push((seed, lit, deep, broken));
        println!("seed {seed}: 亮区 {lit} 节点, 深渊 {deep} 节点, 蚀断 {broken} 线");
    }
    let lits: Vec<usize> = summaries.iter().map(|s| s.1).collect();
    let spread = lits.iter().max().unwrap() - lits.iter().min().unwrap();
    println!("亮区规模跨种子波动: {spread} 节点（>10 = 拓扑显著影响战局）");
}
