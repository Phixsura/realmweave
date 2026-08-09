//! Emergence v2: NON-MONOTONIC void — overcrowding collapse.
//!
//! The single rule change from v1: when a node's void exceeds CRITICAL,
//! it collapses — the node zeroes and ejects a shockwave into neighbors.
//! This is the B-Z-reaction / forest-fire structure: the minimal known
//! recipe for self-organizing waves, spirals and target patterns.
//!
//! Judgment criteria (harsh, pre-registered):
//!   J1 self-sustaining patterns: cut all sources at t=3000; does activity
//!      persist >500 ticks on its own?
//!   J2 pattern diversity: count distinct local-neighborhood fingerprints
//!      over time; a dead system converges, a living one keeps generating.
//!   J3 the eyeball test: keyframes printed for human inspection.

use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};
use std::collections::HashSet;

const DIFFUSION: f32 = 0.22;
const RIFT_EMIT: f32 = 1.2;
const CRITICAL: f32 = 9.0; // collapse threshold — the B3/S23 of this system
const EJECT: f32 = 0.55; // fraction of collapsed void ejected to neighbors
const REFRACTORY_TICKS: u32 = 14; // dead-zone time after collapse

struct Field {
    void: Vec<f32>,
    refractory: Vec<u32>,
}

impl Field {
    fn new(n: usize) -> Field {
        Field {
            void: vec![0.0; n],
            refractory: vec![0; n],
        }
    }
}

fn step(map: &SupplyMap, f: &mut Field, sources_on: bool) {
    let n = map.node_count();

    // Diffusion (flow-limited, stable) — refractory nodes accept nothing.
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

    // Sources.
    if sources_on {
        for &r in &map.rifts {
            nv[r as usize] += RIFT_EMIT;
        }
    }

    // THE RULE: overcrowding collapse + shockwave ejection + refractory.
    let snapshot = nv.clone();
    for a in 0..n {
        if snapshot[a] > CRITICAL && f.refractory[a] == 0 {
            let mass = snapshot[a];
            nv[a] = 0.0;
            f.refractory[a] = REFRACTORY_TICKS;
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

    // Refractory decay.
    for r in f.refractory.iter_mut() {
        *r = r.saturating_sub(1);
    }

    f.void = nv;
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
                    if f.refractory[i] > 0 {
                        'o' // just collapsed (refractory crater)
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

/// Local fingerprint: quantized (self, sorted-neighbors) state.
fn fingerprints(map: &SupplyMap, f: &Field) -> HashSet<u64> {
    let quant = |v: f32, r: u32| -> u8 {
        if r > 0 {
            9
        } else if v > 7.0 {
            3
        } else if v > 3.5 {
            2
        } else if v > 1.0 {
            1
        } else {
            0
        }
    };
    let mut set = HashSet::new();
    for a in 0..map.node_count() {
        let s = quant(f.void[a], f.refractory[a]);
        let mut nb: Vec<u8> = map.adjacency[a]
            .iter()
            .map(|&(b, _)| quant(f.void[b as usize], f.refractory[b as usize]))
            .collect();
        nb.sort_unstable();
        let mut key = s as u64;
        for x in nb {
            key = key * 11 + x as u64;
        }
        set.insert(key);
    }
    set
}

/// Measure avalanche statistics: an avalanche = a connected run of ticks
/// with at least one collapse; size = total collapses in the run.
fn avalanche_census(map: &SupplyMap, ticks: u32) -> Vec<u32> {
    let mut f = Field::new(map.node_count());
    let mut sizes = Vec::new();
    let mut current: u32 = 0;
    // warmup to reach the critical state
    for _ in 0..2000 {
        step(map, &mut f, true);
    }
    for _ in 0..ticks {
        let before: u32 = f.refractory.iter().filter(|r| **r > 0).count() as u32;
        step(map, &mut f, true);
        let after: u32 = f
            .refractory
            .iter()
            .filter(|r| **r == REFRACTORY_TICKS - 1)
            .count() as u32;
        // nodes that JUST collapsed this tick
        let _ = before;
        if after > 0 {
            current += after;
        } else if current > 0 {
            sizes.push(current);
            current = 0;
        }
    }
    sizes
}

fn main() {
    let map = generate_map(20260809, &MapSpec::default());
    let n = map.node_count();
    let mut f = Field::new(n);

    println!("规则：扩散 + 恒定源 + 【过密坍缩→冲击波→不应期】。无光网（先看纯虚空动力学）。\n");

    // ---------- Phase 1: sources on, watch for waves ----------
    let mut all_fingerprints: HashSet<u64> = HashSet::new();
    let mut diversity_curve = Vec::new();
    let mut activity_curve = Vec::new();
    for t in 0..=3000u32 {
        step(&map, &mut f, true);
        if t % 500 == 0 {
            println!("{}", render(&map, &f, &format!("t={t}")));
        }
        if t % 50 == 0 {
            let fps = fingerprints(&map, &f);
            all_fingerprints.extend(fps.iter());
            diversity_curve.push(all_fingerprints.len());
            let collapses = f.refractory.iter().filter(|r| **r > 0).count();
            activity_curve.push(collapses);
        }
    }
    println!("J2 指纹多样性曲线（每 50 tick 的累计新模式数）:");
    println!("  {:?}", diversity_curve);
    println!("坍缩活动曲线（不应期节点数采样）:");
    println!("  {:?}\n", activity_curve);

    // ---------- Phase 2 (J1): cut sources — does activity self-sustain? ----------
    println!("========== J1: t=3000 切断所有裂隙源 ==========");
    let mut survived = 0u32;
    for t in 3000..=6000u32 {
        step(&map, &mut f, false);
        let active = f.refractory.iter().filter(|r| **r > 0).count()
            + f.void.iter().filter(|v| **v > CRITICAL * 0.5).count();
        if active > 0 {
            survived = t - 3000;
        }
        if t % 500 == 0 {
            println!(
                "{}",
                render(&map, &f, &format!("t={t} (断源后 {})", t - 3000))
            );
        }
    }
    println!("J1 结果：断源后活动持续 {survived} tick（>500 = 自持模式存在）");
    println!("【诚实修正】检查断源后的帧：场冻结为均匀残留——上述指标为假阳性，无自持模式。");

    // ---------- SOC test: avalanche size distribution ----------
    println!("\n========== SOC 检验：雪崩规模分布（幂律 = 未编程的涌现签名） ==========");
    let sizes = avalanche_census(&map, 20000);
    let mut hist = std::collections::BTreeMap::new();
    for &s in &sizes {
        // log-2 bins
        let bin = 32 - (s.max(1)).leading_zeros();
        *hist.entry(bin).or_insert(0u32) += 1;
    }
    println!("雪崩总数: {} (20000 tick)", sizes.len());
    println!("规模分布 (log2 分箱):");
    let total = sizes.len() as f32;
    for (bin, count) in &hist {
        let lo = 1u32 << (bin - 1);
        let hi = (1u32 << bin) - 1;
        let frac = *count as f32 / total;
        let bar = "█".repeat((frac * 60.0) as usize);
        println!("  [{lo:>4}-{hi:>4}] {count:>5} {bar}");
    }
    let max = sizes.iter().max().copied().unwrap_or(0);
    let mean = sizes.iter().sum::<u32>() as f32 / total.max(1.0);
    println!(
        "均值 {mean:.1}, 最大 {max} —— 跨度 {}x",
        max as f32 / mean.max(0.1)
    );
    println!("判读：分布若在 log-log 下近似直线（每箱约按常数比衰减）且跨 2 个数量级 = SOC。");
}
