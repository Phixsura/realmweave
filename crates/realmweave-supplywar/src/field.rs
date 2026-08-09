//! The SOC field simulation — production version of the emergence
//! experiments (see examples/emergence{,2,3}.rs and docs).
//!
//! World model:
//! - **Void** is a conserved-inflow scalar field: rifts inject constantly,
//!   it diffuses along graph edges, and only the light network destroys it.
//! - **Collapse (SOC rule)**: a node whose void exceeds CRITICAL collapses —
//!   zeroes, ejects a shockwave into neighbors, and goes refractory.
//!   Avalanche sizes follow a power-law-like distribution (verified):
//!   the world produces earthquakes nobody scripted.
//! - **Light** flows from the core along built links (decaying per hop),
//!   radiates a protective aura, annihilates void with finite throughput,
//!   erodes under deep void, self-repairs where light dominates.
//!
//! Player verbs: BuildLink, Reinforce, Discharge (deliberately trigger a
//! collapse at a chosen node — controlled demolition of pooling void).
//!
//! Determinism: pure function of (map, command stream). No RNG at all in
//! the field itself — the only randomness in the game is the map seed.

use realmweave_core::NodeId;
use serde::{Deserialize, Serialize};

use crate::map::SupplyMap;

pub const TICKS_PER_SEC: u32 = 10;

// --- field constants (drama-zone values from the experiments) ---
pub const DIFFUSION: f32 = 0.22;
pub const RIFT_EMIT: f32 = 1.2;
pub const CRITICAL: f32 = 9.0;
pub const EJECT: f32 = 0.55;
pub const REFRACTORY_TICKS: u32 = 14;
pub const VOID_NODE_CEIL: f32 = 200.0;

pub const CORE_LIGHT: f32 = 35.0;
pub const LIGHT_DECAY: f32 = 0.8;
pub const AURA_DECAY: f32 = 0.25;
pub const KILL_CAP: f32 = 1.1;
pub const ERODE_THRESHOLD: f32 = 1.5;
pub const ERODE_RATE: f32 = 0.04;
pub const REPAIR_RATE: f32 = 0.01;

// --- economy ---
pub const START_ENERGY: f32 = 30.0;
pub const CORE_INCOME: f32 = 1.2; // per second
pub const WELL_INCOME: f32 = 1.0; // per second per lit well
pub const ENERGY_CAP: f32 = 80.0;
pub const COST_BUILD: f32 = 8.0;
pub const COST_REINFORCE: f32 = 10.0;
pub const COST_DISCHARGE: f32 = 12.0;
pub const BUILD_TICKS: u32 = 15;
/// Reinforced links erode at this fraction of the normal rate.
pub const REINFORCED_RESIST: f32 = 0.35;
/// Discharge reach: max graph distance from a lit node.
pub const DISCHARGE_RANGE: u32 = 2;

// --- victory/defeat ---
/// Survive this long (25 min default game).
pub const GAME_LENGTH_TICKS: u64 = 1500 * TICKS_PER_SEC as u64 / 60 * 60; // 15000 = 25min? keep simple:
pub const GAME_LENGTH_SECS: u64 = 900; // 15 minutes for the graybox
/// Defeat: void this deep sitting ON the core.
pub const CORE_DROWN_LEVEL: f32 = 6.0;
/// ... for this long continuously.
pub const CORE_DROWN_TICKS: u32 = 50; // 5 seconds

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    BuildLink(u32),
    Reinforce(u32),
    /// Trigger a controlled collapse at a node (must be within
    /// DISCHARGE_RANGE of a lit node and have void > CRITICAL/2).
    Discharge(NodeId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkState {
    Empty,
    Building(u32),
    Single,
    Reinforced,
    /// Eroded to destruction; rebuildable at full price.
    Broken,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    /// Survived to the horn with >=1 lit well.
    Victory { wells_lit: usize },
    /// The core drowned in void.
    Defeat { tick: u64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FieldEvent {
    /// An avalanche chain ended; total collapsed nodes.
    Avalanche {
        tick: u64,
        size: u32,
    },
    LinkBroken {
        tick: u64,
        edge: u32,
    },
    WellLit {
        tick: u64,
        well: NodeId,
    },
    WellLost {
        tick: u64,
        well: NodeId,
    },
    Discharged {
        tick: u64,
        node: NodeId,
    },
    Ended {
        tick: u64,
        outcome: Outcome,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldState {
    pub tick: u64,
    pub energy: f32,
    pub void: Vec<f32>,
    pub light: Vec<f32>,
    pub refractory: Vec<u32>,
    pub links: Vec<LinkState>,
    /// HP fraction per link (1.0 intact; meaningful for Single/Reinforced).
    pub link_hp: Vec<f32>,
    /// Node on the light network this tick (hard light reaches it).
    pub on_network: Vec<bool>,
    pub outcome: Option<Outcome>,
    pub events: Vec<FieldEvent>,
    /// Current avalanche accumulator (collapses in the ongoing chain).
    avalanche_run: u32,
    /// Consecutive ticks the core has been drowning.
    core_drown_run: u32,
    /// Wells lit last tick (for event edges).
    wells_lit_prev: Vec<bool>,
}

impl FieldState {
    pub fn new(map: &SupplyMap) -> FieldState {
        FieldState {
            tick: 0,
            energy: START_ENERGY,
            void: vec![0.0; map.node_count()],
            light: vec![0.0; map.node_count()],
            refractory: vec![0; map.node_count()],
            links: vec![LinkState::Empty; map.edges.len()],
            link_hp: vec![1.0; map.edges.len()],
            on_network: {
                let mut v = vec![false; map.node_count()];
                v[map.core as usize] = true;
                v
            },
            outcome: None,
            events: Vec::new(),
            avalanche_run: 0,
            core_drown_run: 0,
            wells_lit_prev: vec![false; map.wells.len()],
        }
    }

    pub fn seconds(&self) -> f32 {
        self.tick as f32 / TICKS_PER_SEC as f32
    }

    pub fn is_link_alive(&self, e: u32) -> bool {
        matches!(
            self.links[e as usize],
            LinkState::Single | LinkState::Reinforced
        ) && self.link_hp[e as usize] > 0.0
    }

    pub fn income_per_sec(&self, map: &SupplyMap) -> f32 {
        let wells = map
            .wells
            .iter()
            .filter(|&&w| self.on_network[w as usize])
            .count() as f32;
        CORE_INCOME + WELL_INCOME * wells
    }
}

/// Advance one tick: player commands first, then the field.
pub fn tick(map: &SupplyMap, s: &mut FieldState, commands: &[Command]) {
    if s.outcome.is_some() {
        return;
    }
    s.tick += 1;
    let n = map.node_count();

    // ------------------------------------------------- player commands ---
    for cmd in commands {
        apply(map, s, *cmd);
    }

    // ------------------------------------------------------ light pass ---
    let mut light = vec![0.0f32; n];
    light[map.core as usize] = CORE_LIGHT;
    let mut order = vec![map.core];
    let mut seen = vec![false; n];
    seen[map.core as usize] = true;
    let mut i = 0;
    while i < order.len() {
        let cur = order[i];
        i += 1;
        let outs: Vec<(NodeId, u32)> = map.adjacency[cur as usize]
            .iter()
            .copied()
            .filter(|(nb, e)| s.is_link_alive(*e) && !seen[*nb as usize])
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

    // aura
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

    // -------------------------------------------------- construction ---
    let mut refresh = false;
    for link in s.links.iter_mut() {
        if let LinkState::Building(t) = link {
            *t -= 1;
            if *t == 0 {
                *link = LinkState::Single;
                refresh = true;
            }
        }
    }
    let _ = refresh; // network recomputes every tick anyway

    // ------------------------------------------------- void dynamics ---
    let mut nv = s.void.clone();
    for a in 0..n {
        let va = s.void[a];
        if va <= 0.0 {
            continue;
        }
        let lower: Vec<(usize, f32)> = map.adjacency[a]
            .iter()
            .map(|&(b, _)| (b as usize, va - s.void[b as usize]))
            .filter(|(b, g)| *g > 0.0 && s.refractory[*b] == 0)
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
        nv[r as usize] = (nv[r as usize] + RIFT_EMIT).min(VOID_NODE_CEIL);
    }

    // annihilation (hard light, finite throughput)
    for i in 0..n {
        if !on_network[i] {
            continue;
        }
        let kill = light[i].min(nv[i]).min(KILL_CAP);
        nv[i] = (nv[i] - kill).max(0.0);
        light[i] = (light[i] - kill).max(0.0);
    }

    // SOC collapse
    let mut collapses = 0u32;
    let snapshot = nv.clone();
    for a in 0..n {
        if snapshot[a] > CRITICAL && s.refractory[a] == 0 {
            let mass = snapshot[a];
            nv[a] = 0.0;
            s.refractory[a] = REFRACTORY_TICKS;
            collapses += 1;
            let nbs: Vec<usize> = map.adjacency[a]
                .iter()
                .map(|&(b, _)| b as usize)
                .filter(|&b| s.refractory[b] == 0)
                .collect();
            if !nbs.is_empty() {
                let share = mass * EJECT / nbs.len() as f32;
                for b in nbs {
                    nv[b] += share;
                }
            }
        }
    }
    for r in s.refractory.iter_mut() {
        *r = r.saturating_sub(1);
    }

    // avalanche bookkeeping
    if collapses > 0 {
        s.avalanche_run += collapses;
    } else if s.avalanche_run > 0 {
        let size = s.avalanche_run;
        s.avalanche_run = 0;
        s.events.push(FieldEvent::Avalanche { tick: s.tick, size });
    }

    // erosion / repair
    for e in 0..map.edges.len() {
        let alive = matches!(s.links[e], LinkState::Single | LinkState::Reinforced);
        if !alive || s.link_hp[e] <= 0.0 {
            continue;
        }
        let (a, b) = map.edge_endpoints(e as u32);
        let (a, b) = (a as usize, b as usize);
        let v = nv[a].max(nv[b]);
        let l = light[a].min(light[b]);
        if v > ERODE_THRESHOLD {
            let resist = if s.links[e] == LinkState::Reinforced {
                REINFORCED_RESIST
            } else {
                1.0
            };
            s.link_hp[e] -= ERODE_RATE * (v - ERODE_THRESHOLD) * resist;
            if s.link_hp[e] <= 0.0 {
                s.links[e] = LinkState::Broken;
                s.link_hp[e] = 0.0;
                s.events.push(FieldEvent::LinkBroken {
                    tick: s.tick,
                    edge: e as u32,
                });
            }
        } else if l > v && s.link_hp[e] < 1.0 {
            s.link_hp[e] = (s.link_hp[e] + REPAIR_RATE).min(1.0);
        }
    }

    // ------------------------------------------------------- economy ---
    s.void = nv;
    s.light = light;
    s.on_network = on_network;
    let income = s.income_per_sec(map) / TICKS_PER_SEC as f32;
    s.energy = (s.energy + income).min(ENERGY_CAP);

    // well lit/lost events
    for (i, &w) in map.wells.iter().enumerate() {
        let now = s.on_network[w as usize];
        if now && !s.wells_lit_prev[i] {
            s.events.push(FieldEvent::WellLit {
                tick: s.tick,
                well: w,
            });
        } else if !now && s.wells_lit_prev[i] {
            s.events.push(FieldEvent::WellLost {
                tick: s.tick,
                well: w,
            });
        }
        s.wells_lit_prev[i] = now;
    }

    // ------------------------------------------------------- outcome ---
    if s.void[map.core as usize] > CORE_DROWN_LEVEL {
        s.core_drown_run += 1;
    } else {
        s.core_drown_run = 0;
    }
    if s.core_drown_run >= CORE_DROWN_TICKS {
        let outcome = Outcome::Defeat { tick: s.tick };
        s.outcome = Some(outcome);
        s.events.push(FieldEvent::Ended {
            tick: s.tick,
            outcome,
        });
        return;
    }
    if s.tick >= GAME_LENGTH_SECS * TICKS_PER_SEC as u64 {
        let wells_lit = map
            .wells
            .iter()
            .filter(|&&w| s.on_network[w as usize])
            .count();
        let outcome = if wells_lit >= 1 {
            Outcome::Victory { wells_lit }
        } else {
            Outcome::Defeat { tick: s.tick }
        };
        s.outcome = Some(outcome);
        s.events.push(FieldEvent::Ended {
            tick: s.tick,
            outcome,
        });
    }
}

fn apply(map: &SupplyMap, s: &mut FieldState, cmd: Command) {
    match cmd {
        Command::BuildLink(e) => {
            if e as usize >= s.links.len() {
                return;
            }
            let buildable = matches!(s.links[e as usize], LinkState::Empty | LinkState::Broken);
            if !buildable || s.energy < COST_BUILD {
                return;
            }
            let (a, b) = map.edge_endpoints(e);
            if !s.on_network[a as usize] && !s.on_network[b as usize] {
                return;
            }
            s.energy -= COST_BUILD;
            s.links[e as usize] = LinkState::Building(BUILD_TICKS);
            s.link_hp[e as usize] = 1.0;
        }
        Command::Reinforce(e) => {
            if e as usize >= s.links.len() {
                return;
            }
            if s.links[e as usize] != LinkState::Single || s.energy < COST_REINFORCE {
                return;
            }
            s.energy -= COST_REINFORCE;
            s.links[e as usize] = LinkState::Reinforced;
        }
        Command::Discharge(node) => {
            let ni = node as usize;
            if ni >= s.void.len() || s.energy < COST_DISCHARGE {
                return;
            }
            // must have meaningful void and be near the network
            if s.void[ni] < CRITICAL / 2.0 || s.refractory[ni] > 0 {
                return;
            }
            let d = map.distances(node);
            let near = (0..map.node_count())
                .any(|i| s.on_network[i] && d[i].unwrap_or(99) <= DISCHARGE_RANGE);
            if !near {
                return;
            }
            s.energy -= COST_DISCHARGE;
            // controlled collapse: same physics as natural, so chains can
            // trigger — discharging a deep pool can start an avalanche.
            let mass = s.void[ni];
            s.void[ni] = 0.0;
            s.refractory[ni] = REFRACTORY_TICKS;
            let nbs: Vec<usize> = map.adjacency[ni]
                .iter()
                .map(|&(b, _)| b as usize)
                .filter(|&b| s.refractory[b] == 0)
                .collect();
            if !nbs.is_empty() {
                let share = mass * EJECT / nbs.len() as f32;
                for b in nbs {
                    s.void[b] += share;
                }
            }
            s.events.push(FieldEvent::Discharged { tick: s.tick, node });
        }
    }
}

/// Run a full game from a command log (replay/testing).
pub fn run(map: &SupplyMap, commands: &[(u64, Command)], max_ticks: u64) -> FieldState {
    let mut s = FieldState::new(map);
    let mut idx = 0;
    let mut buf = Vec::new();
    while s.outcome.is_none() && s.tick < max_ticks {
        buf.clear();
        while idx < commands.len() && commands[idx].0 == s.tick + 1 {
            buf.push(commands[idx].1);
            idx += 1;
        }
        tick(map, &mut s, &buf);
    }
    s
}
