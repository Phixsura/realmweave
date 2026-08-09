//! Observe the production field for emergence-killers: does it settle into
//! a static equilibrium after the opening?
use realmweave_supplywar::field::{self, Command, FieldState, LinkState};
use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};

fn autopilot(map: &SupplyMap, s: &FieldState) -> Vec<Command> {
    let mut cmds = Vec::new();
    let mut budget = s.energy;
    let dist = map.distances(map.core);
    let mut wells: Vec<_> = map.wells.clone();
    wells.sort_by_key(|&w| dist[w as usize].unwrap_or(999));
    let mut trunk: Vec<u32> = Vec::new();
    for &w in wells.iter().take(5) {
        let mut cur = w;
        while cur != map.core {
            let d = dist[cur as usize].unwrap();
            if let Some(&(prev, edge)) = map.adjacency[cur as usize]
                .iter()
                .find(|(n, _)| dist[*n as usize] == Some(d - 1))
            {
                trunk.push(edge);
                cur = prev;
            } else {
                break;
            }
        }
    }
    trunk.sort_unstable();
    trunk.dedup();
    for &e in &trunk {
        if budget < field::COST_BUILD {
            break;
        }
        if matches!(s.links[e as usize], LinkState::Empty | LinkState::Broken) {
            let (a, b) = map.edge_endpoints(e);
            if s.on_network[a as usize] || s.on_network[b as usize] {
                cmds.push(Command::BuildLink(e));
                budget -= field::COST_BUILD;
            }
        }
    }
    cmds
}

fn main() {
    let map = generate_map(20260809, &MapSpec::default());
    let mut s = FieldState::new(&map);
    let mut last_events = 0;
    println!("t(s) | 雪崩(30s内) | 最大 | 虚空总量 | 光总量 | 蚀断 | 场变化率");
    let mut prev_void: Vec<f32> = s.void.clone();
    for t in 0..9000u64 {
        let cmds = autopilot(&map, &s);
        field::tick(&map, &mut s, &cmds);
        if t % 300 == 299 {
            let quakes: Vec<u32> = s.events[last_events..]
                .iter()
                .filter_map(|e| match e {
                    field::FieldEvent::Avalanche { size, .. } => Some(*size),
                    _ => None,
                })
                .collect();
            last_events = s.events.len();
            let vt: f32 = s.void.iter().sum();
            let lt: f32 = s.light.iter().sum();
            let broken = s.links.iter().filter(|l| **l == LinkState::Broken).count();
            // field change rate: L1 distance between void now and 30s ago
            let delta: f32 = s
                .void
                .iter()
                .zip(prev_void.iter())
                .map(|(a, b)| (a - b).abs())
                .sum();
            prev_void = s.void.clone();
            println!(
                "{:>4} | {:>10} | {:>4} | {:>8.0} | {:>6.0} | {:>4} | {:>8.1}",
                (t + 1) / 10,
                quakes.len(),
                quakes.iter().max().unwrap_or(&0),
                vt,
                lt,
                broken,
                delta
            );
        }
        if s.outcome.is_some() {
            println!("outcome: {:?}", s.outcome);
            break;
        }
    }
}
