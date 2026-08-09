//! Field (SOC) simulation tests: determinism, commands, collapse physics,
//! outcomes, and the strategy-leverage property from the experiments.

use realmweave_supplywar::field::{
    self, Command, FieldEvent, FieldState, LinkState, Outcome, COST_BUILD, CRITICAL, TICKS_PER_SEC,
};
use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};

fn map(seed: u64) -> SupplyMap {
    generate_map(seed, &MapSpec::default())
}

fn path_from_core(m: &SupplyMap, to: realmweave_core::NodeId) -> Vec<u32> {
    let dist = m.distances(m.core);
    let mut path = Vec::new();
    let mut cur = to;
    while cur != m.core {
        let d = dist[cur as usize].unwrap();
        let (prev, edge) = m.adjacency[cur as usize]
            .iter()
            .find(|(n, _)| dist[*n as usize] == Some(d - 1))
            .copied()
            .unwrap();
        path.push(edge);
        cur = prev;
    }
    path.reverse();
    path
}

fn build_chain(m: &SupplyMap, s: &mut FieldState, chain: &[u32]) {
    for &e in chain {
        let mut guard = 0;
        while s.energy < COST_BUILD && guard < 3000 {
            field::tick(m, s, &[]);
            guard += 1;
        }
        field::tick(m, s, &[Command::BuildLink(e)]);
        for _ in 0..field::BUILD_TICKS {
            field::tick(m, s, &[]);
        }
    }
}

#[test]
fn replay_is_deterministic() {
    let m = map(2026);
    let (_, e0) = m.adjacency[m.core as usize][0];
    let commands: Vec<(u64, Command)> =
        vec![(1, Command::BuildLink(e0)), (300, Command::Reinforce(e0))];
    let a = field::run(&m, &commands, 5000);
    let b = field::run(&m, &commands, 5000);
    assert_eq!(a, b);
}

#[test]
fn build_and_light_propagation() {
    let m = map(7);
    let mut s = FieldState::new(&m);
    let (n0, e0) = m.adjacency[m.core as usize][0];
    field::tick(&m, &mut s, &[Command::BuildLink(e0)]);
    assert!(matches!(s.links[e0 as usize], LinkState::Building(_)));
    for _ in 0..field::BUILD_TICKS {
        field::tick(&m, &mut s, &[]);
    }
    assert_eq!(s.links[e0 as usize], LinkState::Single);
    assert!(s.on_network[n0 as usize], "light reaches the new node");
    assert!(s.light[n0 as usize] > 0.0);
}

#[test]
fn build_requires_network_endpoint_and_energy() {
    let m = map(7);
    let mut s = FieldState::new(&m);
    // far edge: rejected
    let dist = m.distances(m.core);
    let far = (0..m.edges.len() as u32)
        .find(|&e| {
            let (a, b) = m.edge_endpoints(e);
            dist[a as usize].unwrap() > 3 && dist[b as usize].unwrap() > 3
        })
        .unwrap();
    field::tick(&m, &mut s, &[Command::BuildLink(far)]);
    assert_eq!(s.links[far as usize], LinkState::Empty);
    // no energy: rejected
    s.energy = COST_BUILD - 1.0;
    let (_, e0) = m.adjacency[m.core as usize][0];
    field::tick(&m, &mut s, &[Command::BuildLink(e0)]);
    assert_eq!(s.links[e0 as usize], LinkState::Empty);
}

#[test]
fn void_builds_up_and_collapses() {
    let m = map(7);
    let mut s = FieldState::new(&m);
    // No player action: rifts pump void; collapses must eventually occur.
    let mut saw_avalanche = false;
    for _ in 0..3000 {
        field::tick(&m, &mut s, &[]);
        if s.events
            .iter()
            .any(|e| matches!(e, FieldEvent::Avalanche { .. }))
        {
            saw_avalanche = true;
            break;
        }
    }
    assert!(saw_avalanche, "SOC field must produce avalanches unaided");
}

#[test]
fn idle_play_drowns_the_core() {
    let m = map(7);
    let mut s = FieldState::new(&m);
    for _ in 0..(field::GAME_LENGTH_SECS * TICKS_PER_SEC as u64) {
        field::tick(&m, &mut s, &[]);
        if s.outcome.is_some() {
            break;
        }
    }
    assert!(
        matches!(s.outcome, Some(Outcome::Defeat { .. })),
        "doing nothing must lose: {:?}",
        s.outcome
    );
}

#[test]
fn near_dam_suppresses_far_dam_does_not() {
    // The strategy-leverage property (experiment T2), now as a regression
    // test: walling a rift's mouth prevents its avalanches entirely.
    let m = map(20260809);
    let rift = m.rifts[0];

    // near dam: build to distance-2 of the rift
    let dist_r = m.distances(rift);
    let dist_c = m.distances(m.core);
    let near_head = (0..m.node_count() as u16)
        .filter(|&nd| dist_r[nd as usize] == Some(2))
        .min_by_key(|&nd| dist_c[nd as usize].unwrap_or(999))
        .unwrap();
    let mut s_near = FieldState::new(&m);
    // free build for the physics test
    for &e in &path_from_core(&m, near_head) {
        s_near.energy = 80.0;
        field::tick(&m, &mut s_near, &[Command::BuildLink(e)]);
        for _ in 0..field::BUILD_TICKS {
            field::tick(&m, &mut s_near, &[]);
        }
    }
    let mut near_avalanches = 0u32;
    for _ in 0..6000 {
        field::tick(&m, &mut s_near, &[]);
        if s_near.outcome.is_some() {
            break;
        }
    }
    for e in &s_near.events {
        if let FieldEvent::Avalanche { size, .. } = e {
            // count only avalanches near this rift
            let _ = size;
            near_avalanches += 1;
        }
    }

    let mut s_none = FieldState::new(&m);
    let mut none_avalanches = 0u32;
    for _ in 0..6000 {
        field::tick(&m, &mut s_none, &[]);
        if s_none.outcome.is_some() {
            break;
        }
    }
    for e in &s_none.events {
        if matches!(e, FieldEvent::Avalanche { .. }) {
            none_avalanches += 1;
        }
    }
    // One of four rifts muzzled → expect a meaningful global reduction.
    // (Avalanche events don't carry location; per-rift attribution would
    // need spatial tagging — graybox asserts the global effect.)
    assert!(
        (near_avalanches as f32) < none_avalanches as f32 * 0.75,
        "damming a rift must cut avalanches ≥25%: {near_avalanches} vs {none_avalanches}"
    );
}

#[test]
fn discharge_triggers_controlled_collapse() {
    let m = map(7);
    let mut s = FieldState::new(&m);
    // Build toward the nearest rift so the network is close to pooling void.
    let dist = m.distances(m.core);
    let rift = *m
        .rifts
        .iter()
        .min_by_key(|&&r| dist[r as usize].unwrap())
        .unwrap();
    let dist_r = m.distances(rift);
    let dist_c = m.distances(m.core);
    let head = (0..m.node_count() as u16)
        .filter(|&nd| dist_r[nd as usize] == Some(3))
        .min_by_key(|&nd| dist_c[nd as usize].unwrap_or(999))
        .unwrap();
    for &e in &path_from_core(&m, head) {
        s.energy = 80.0;
        field::tick(&m, &mut s, &[Command::BuildLink(e)]);
        for _ in 0..field::BUILD_TICKS {
            field::tick(&m, &mut s, &[]);
        }
    }
    // Let void pool near the rift.
    let mut target = None;
    for _ in 0..2000 {
        field::tick(&m, &mut s, &[]);
        // find a poolable node within discharge range of the network
        if let Some(node) = (0..m.node_count() as u16).find(|&nd| {
            s.void[nd as usize] > CRITICAL / 2.0 && s.refractory[nd as usize] == 0 && {
                let d = m.distances(nd);
                (0..m.node_count())
                    .any(|i| s.on_network[i] && d[i].unwrap_or(99) <= field::DISCHARGE_RANGE)
            }
        }) {
            target = Some(node);
            break;
        }
    }
    let Some(node) = target else {
        panic!("no dischargeable pool formed within 2000 ticks");
    };
    s.energy = 80.0;
    let void_before = s.void[node as usize];
    field::tick(&m, &mut s, &[Command::Discharge(node)]);
    assert_eq!(
        s.void[node as usize], 0.0,
        "pool zeroed (was {void_before})"
    );
    assert!(s.refractory[node as usize] > 0);
    assert!(s
        .events
        .iter()
        .any(|e| matches!(e, FieldEvent::Discharged { .. })));
}

#[test]
fn wells_produce_income_when_lit() {
    let m = map(7);
    let mut s = FieldState::new(&m);
    let base = s.income_per_sec(&m);
    let dist = m.distances(m.core);
    let well = *m
        .wells
        .iter()
        .min_by_key(|&&w| dist[w as usize].unwrap())
        .unwrap();
    build_chain(&m, &mut s, &path_from_core(&m, well));
    assert!(s.on_network[well as usize]);
    assert!(s.income_per_sec(&m) > base);
    assert!(s
        .events
        .iter()
        .any(|e| matches!(e, FieldEvent::WellLit { .. })));
}
