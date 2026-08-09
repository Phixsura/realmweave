//! Parameter sweep: find the live zone of the void/light CA.
//! Interesting = fronts oscillate, some links break (but not all),
//! and outcomes vary across seeds.
use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};

#[derive(Clone, Copy)]
struct Params {
    diffusion: f32,
    rift_emit: f32,
    core_light: f32,
    light_decay: f32,
    erode_threshold: f32,
    erode_rate: f32,
}

#[derive(Clone)]
struct Field {
    void: Vec<f32>,
    light: Vec<f32>,
    link_hp: Vec<f32>,
    built: Vec<bool>,
}

fn step(map: &SupplyMap, f: &mut Field, p: Params) {
    let n = map.node_count();
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
    let mut nv = f.void.clone();
    for a in 0..n {
        for &(b, _) in &map.adjacency[a] {
            let b = b as usize;
            if a < b {
                let d = (f.void[a] - f.void[b]) * p.diffusion;
                nv[a] -= d;
                nv[b] += d;
            }
        }
    }
    for &r in &map.rifts {
        nv[r as usize] = (nv[r as usize] + p.rift_emit).min(60.0);
    }
    for i in 0..n {
        let kill = light[i].min(nv[i]);
        nv[i] = (nv[i] - kill).max(0.0);
        light[i] = (light[i] - kill).max(0.0);
    }
    for e in 0..map.edges.len() {
        if !f.built[e] || f.link_hp[e] <= 0.0 {
            continue;
        }
        let (a, b) = map.edge_endpoints(e as u32);
        let v = nv[a as usize].max(nv[b as usize]);
        if v > p.erode_threshold {
            f.link_hp[e] -= p.erode_rate * (v - p.erode_threshold);
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
    // Run on 3 seeds; measure: front movement (lit count variance over
    // time), broken fraction (want 0.1..0.6), seed spread.
    let mut lit_finals = Vec::new();
    let mut total_move = 0.0f32;
    let mut broken_fracs = Vec::new();
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
        let mut lit_series = Vec::new();
        for t in 0..2000 {
            step(&map, &mut f, p);
            if t % 100 == 0 {
                lit_series.push(f.light.iter().filter(|l| **l > 0.5).count() as f32);
            }
        }
        // movement = sum of |Δlit| between samples after warmup
        for w in lit_series.windows(2).skip(3) {
            total_move += (w[1] - w[0]).abs();
        }
        let built_n = f.built.iter().filter(|b| **b).count().max(1);
        let broken = f
            .built
            .iter()
            .zip(f.link_hp.iter())
            .filter(|(b, hp)| **b && **hp <= 0.0)
            .count();
        broken_fracs.push(broken as f32 / built_n as f32);
        lit_finals.push(f.light.iter().filter(|l| **l > 0.5).count() as f32);
    }
    let spread = lit_finals.iter().cloned().fold(f32::MIN, f32::max)
        - lit_finals.iter().cloned().fold(f32::MAX, f32::min);
    let mean_broken = broken_fracs.iter().sum::<f32>() / broken_fracs.len() as f32;
    // Score: movement + spread bonus + broken in sweet band
    let broken_score = if mean_broken > 0.05 && mean_broken < 0.7 {
        20.0
    } else {
        0.0
    };
    let score = total_move + spread * 2.0 + broken_score;
    (
        score,
        format!("move={total_move:.0} spread={spread:.0} broken={mean_broken:.2}"),
    )
}

fn main() {
    let mut best: Vec<(f32, String, String)> = Vec::new();
    for &diffusion in &[0.12, 0.2, 0.3] {
        for &rift_emit in &[0.8, 1.5, 2.5] {
            for &core_light in &[10.0, 18.0, 30.0] {
                for &light_decay in &[0.7, 0.85] {
                    for &erode_rate in &[0.05, 0.15, 0.4] {
                        let p = Params {
                            diffusion,
                            rift_emit,
                            core_light,
                            light_decay,
                            erode_threshold: 1.5,
                            erode_rate,
                        };
                        let (score, detail) = evaluate(p);
                        best.push((score, detail,
                            format!("dif={diffusion} emit={rift_emit} light={core_light} decay={light_decay} erode={erode_rate}")));
                    }
                }
            }
        }
    }
    best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("Top 8 parameter sets (score = front movement + topology spread + healthy breakage):");
    for (score, detail, params) in best.iter().take(8) {
        println!("  [{score:>7.1}] {detail}  <<{params}>>");
    }
    println!("\nBottom 3 (for contrast):");
    for (score, detail, params) in best.iter().rev().take(3) {
        println!("  [{score:>7.1}] {detail}  <<{params}>>");
    }
}
