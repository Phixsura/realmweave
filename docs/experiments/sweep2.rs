//! Sweep v2 with Rule 4: light aura + self-repair — searching for a LIVING
//! front (moves, oscillates, sometimes breaks through, topology-dependent).
use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};

#[derive(Clone, Copy)]
struct Params {
    diffusion: f32,
    rift_emit: f32,
    core_light: f32,
    light_decay: f32,
    aura_decay: f32,
    erode_rate: f32,
    repair_rate: f32,
    kill_cap: f32,
}

struct Field {
    void: Vec<f32>,
    light: Vec<f32>,
    link_hp: Vec<f32>,
    built: Vec<bool>,
}

fn step(map: &SupplyMap, f: &mut Field, p: Params) {
    let n = map.node_count();
    // network light
    let mut light = vec![0.0f32; n];
    light[map.core as usize] = p.core_light;
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
        let share = light[cur as usize] * p.light_decay / outs.len() as f32;
        for (nb, _) in outs {
            light[nb as usize] += share;
            seen[nb as usize] = true;
            order.push(nb);
        }
    }
    // (seen == on-network set)
    // Rule 4a: aura — two smoothing passes radiate light beyond the network
    for _ in 0..2 {
        let snapshot = light.clone();
        for a in 0..n {
            for &(b, _) in &map.adjacency[a] {
                let b = b as usize;
                let flow = snapshot[a] * p.aura_decay / 6.0;
                if snapshot[a] > snapshot[b] {
                    light[b] += flow;
                }
            }
        }
    }
    // void diffusion + source
    let on_network = seen.clone();
    let mut nv = f.void.clone();
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
        let tg: f32 = lower.iter().map(|(_, g)| g).sum();
        if tg <= 0.0 {
            continue;
        }
        let outflow = (va * p.diffusion).min(tg * 0.5);
        for (b, g) in lower {
            let share = outflow * g / tg;
            nv[a] -= share;
            nv[b] += share;
        }
    }
    for &r in &map.rifts {
        nv[r as usize] = (nv[r as usize] + p.rift_emit).min(200.0);
    }
    // annihilation: hard light only, finite throughput (kill_cap via repair_rate slot? no—add param)
    for i in 0..n {
        if !on_network[i] {
            continue;
        }
        let kill = light[i].min(nv[i]).min(p.kill_cap);
        nv[i] = (nv[i] - kill).max(0.0);
        light[i] = (light[i] - kill).max(0.0);
    }
    // erosion + Rule 4b: repair
    for e in 0..map.edges.len() {
        if !f.built[e] {
            continue;
        }
        let (a, b) = map.edge_endpoints(e as u32);
        let (a, b) = (a as usize, b as usize);
        let v = nv[a].max(nv[b]);
        let l = light[a].min(light[b]);
        if v > 1.5 && f.link_hp[e] > 0.0 {
            f.link_hp[e] -= p.erode_rate * (v - 1.5);
        } else if l > v && f.link_hp[e] < 1.0 {
            f.link_hp[e] = (f.link_hp[e] + p.repair_rate).min(1.0);
        }
    }
    f.void = nv;
    f.light = light;
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

fn evaluate(p: Params) -> (f32, String) {
    let mut lit_finals = Vec::new();
    let mut total_move = 0.0f32;
    let mut osc = 0.0f32; // direction changes = front oscillation
    let mut mean_broken = 0.0f32;
    for seed in [1u64, 2, 3] {
        let map = generate_map(seed, &MapSpec::default());
        let mut f = Field {
            void: vec![0.0; map.node_count()],
            light: vec![0.0; map.node_count()],
            link_hp: vec![1.0; map.edges.len()],
            built: vec![false; map.edges.len()],
        };
        let d = map.distances(map.core);
        let targets: Vec<u16> = (0..map.node_count() as u16)
            .filter(|&nd| d[nd as usize] == Some(4))
            .take(3)
            .collect();
        for &t in &targets {
            build_path(&map, &mut f, t);
        }
        let mut series = Vec::new();
        for t in 0..2500 {
            step(&map, &mut f, p);
            if t % 100 == 0 {
                series.push(f.light.iter().filter(|l| **l > 0.5).count() as f32);
            }
        }
        let mut last_dir = 0.0f32;
        for w in series.windows(2).skip(3) {
            let d = w[1] - w[0];
            total_move += d.abs();
            if d * last_dir < 0.0 {
                osc += 1.0;
            }
            if d != 0.0 {
                last_dir = d;
            }
        }
        let built_n = f.built.iter().filter(|b| **b).count().max(1);
        let broken = f
            .built
            .iter()
            .zip(f.link_hp.iter())
            .filter(|(b, hp)| **b && **hp <= 0.0)
            .count();
        mean_broken += broken as f32 / built_n as f32 / 3.0;
        lit_finals.push(*series.last().unwrap());
    }
    let spread = lit_finals.iter().cloned().fold(f32::MIN, f32::max)
        - lit_finals.iter().cloned().fold(f32::MAX, f32::min);
    // alive = it moves AND oscillates AND doesn't fully die or fully win
    let survival_ok = lit_finals.iter().all(|&l| l > 2.0);
    let contested = mean_broken > 0.02 && mean_broken < 0.8;
    let score = total_move
        + osc * 8.0
        + spread * 2.0
        + if survival_ok { 15.0 } else { 0.0 }
        + if contested { 25.0 } else { 0.0 };
    (score, format!("move={total_move:.0} osc={osc:.0} spread={spread:.0} broken={mean_broken:.2} alive={survival_ok}"))
}

fn main() {
    let mut best: Vec<(f32, String, String)> = Vec::new();
    for &diffusion in &[0.2] {
        for &rift_emit in &[0.8, 1.0, 1.3] {
            for &core_light in &[35.0, 45.0] {
                for &kill_cap in &[0.7, 0.9, 1.1, 1.4] {
                    for &erode_rate in &[0.04, 0.08] {
                        for &repair_rate in &[0.01] {
                            let p = Params {
                                diffusion,
                                rift_emit,
                                core_light,
                                light_decay: 0.8,
                                aura_decay: 0.25,
                                erode_rate,
                                repair_rate,
                                kill_cap,
                            };
                            let (score, detail) = evaluate(p);
                            best.push((
                                score,
                                detail,
                                format!(
                                "emit={rift_emit} L={core_light} cap={kill_cap} er={erode_rate}"),
                            ));
                        }
                    }
                }
            }
        }
    }
    best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("Top 10:");
    for (s, d, p) in best.iter().take(10) {
        println!("  [{s:>7.1}] {d}  <<{p}>>");
    }
}
