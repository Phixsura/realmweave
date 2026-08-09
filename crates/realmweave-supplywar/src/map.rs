//! Map generation: single-layer hex with carved holes, core, wells, rifts.
//!
//! Generation is seed-deterministic and self-validating: constraint failures
//! reroll internally (bounded attempts) so a returned map is always valid.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use realmweave_core::boardgen::{hex_coords, hex_distance, HEX_DIRS};
use realmweave_core::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapSpec {
    /// Hex radius of the base disc (design: 4 → 61 nodes before carving).
    pub radius: i32,
    /// Nodes to carve out (design: 8..=12).
    pub carve_min: usize,
    pub carve_max: usize,
    pub wells: usize,
    pub rifts: usize,
}

impl Default for MapSpec {
    fn default() -> Self {
        // 300-action redesign: radius 5 (91 nodes, ~240 edges) gives the
        // build/reinforce budget a 300-action game needs; 8 wells spread
        // work across the map; 4 rifts sustain multi-front pressure.
        MapSpec {
            radius: 5,
            carve_min: 12,
            carve_max: 18,
            wells: 8,
            rifts: 4,
        }
    }
}

/// A generated single-layer playfield. Node/edge indices are dense and
/// stable for the whole game; carving happens before indexing so there are
/// no ghost nodes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupplyMap {
    pub seed: u64,
    /// Axial coordinate per node (render hint + debugging).
    pub axial: Vec<[i32; 2]>,
    /// Undirected edges as (a, b) node-index pairs, a < b.
    pub edges: Vec<(NodeId, NodeId)>,
    /// Adjacency: node -> (neighbor, edge index).
    pub adjacency: Vec<Vec<(NodeId, u32)>>,
    pub core: NodeId,
    pub wells: Vec<NodeId>,
    pub rifts: Vec<NodeId>,
}

impl SupplyMap {
    pub fn node_count(&self) -> usize {
        self.axial.len()
    }

    pub fn edge_endpoints(&self, edge: u32) -> (NodeId, NodeId) {
        self.edges[edge as usize]
    }

    /// Edge index between two nodes, if adjacent.
    pub fn edge_between(&self, a: NodeId, b: NodeId) -> Option<u32> {
        self.adjacency[a as usize]
            .iter()
            .find(|(n, _)| *n == b)
            .map(|(_, e)| *e)
    }

    /// BFS graph distances from a node (full map, ignoring game state).
    pub fn distances(&self, from: NodeId) -> Vec<Option<u32>> {
        let mut dist = vec![None; self.node_count()];
        let mut queue = VecDeque::new();
        dist[from as usize] = Some(0);
        queue.push_back(from);
        while let Some(cur) = queue.pop_front() {
            let d = dist[cur as usize].unwrap();
            for &(next, _) in &self.adjacency[cur as usize] {
                if dist[next as usize].is_none() {
                    dist[next as usize] = Some(d + 1);
                    queue.push_back(next);
                }
            }
        }
        dist
    }
}

/// Generate a valid map for `seed`. Internal rerolls (different sub-seeds)
/// guarantee the returned map satisfies all constraints from design §2.
pub fn generate_map(seed: u64, spec: &MapSpec) -> SupplyMap {
    for attempt in 0u64..64 {
        let sub_seed = seed.wrapping_add(attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        if let Some(map) = try_generate(seed, sub_seed, spec) {
            return map;
        }
    }
    panic!("map generation failed after 64 attempts (spec unsatisfiable?)");
}

fn try_generate(public_seed: u64, sub_seed: u64, spec: &MapSpec) -> Option<SupplyMap> {
    let mut rng = StdRng::seed_from_u64(sub_seed);
    let coords = hex_coords(spec.radius);

    // --- carve holes ---
    let carve_n = rng.gen_range(spec.carve_min..=spec.carve_max);
    // Core candidate: westmost coord (min x in axial->cartesian ~ q + r/2).
    let west = |ax: &[i32; 2]| ax[0] as f32 + ax[1] as f32 / 2.0;
    let core_ax = *coords
        .iter()
        .min_by(|a, b| west(a).partial_cmp(&west(b)).unwrap())
        .unwrap();
    // Rifts: east rim (max west()); protected from carving.
    let mut east_rim: Vec<[i32; 2]> = coords
        .iter()
        .copied()
        .filter(|ax| hex_distance(*ax, [0, 0]) == spec.radius && west(ax) > 0.0)
        .collect();
    east_rim.sort_by(|a, b| west(b).partial_cmp(&west(a)).unwrap());

    let mut protected: Vec<[i32; 2]> = vec![core_ax];
    protected.extend(east_rim.iter().take(6).copied());

    let mut carvable: Vec<[i32; 2]> = coords
        .iter()
        .copied()
        .filter(|ax| !protected.contains(ax))
        .collect();
    carvable.shuffle(&mut rng);
    let carved: Vec<[i32; 2]> = carvable.into_iter().take(carve_n).collect();

    let kept: Vec<[i32; 2]> = coords
        .iter()
        .copied()
        .filter(|ax| !carved.contains(ax))
        .collect();

    // --- index nodes & edges ---
    let index_of = |ax: [i32; 2]| kept.iter().position(|k| *k == ax);
    let mut edges: Vec<(NodeId, NodeId)> = Vec::new();
    for (i, &ax) in kept.iter().enumerate() {
        for d in HEX_DIRS {
            let nb = [ax[0] + d[0], ax[1] + d[1]];
            if let Some(j) = index_of(nb) {
                if i < j {
                    edges.push((i as NodeId, j as NodeId));
                }
            }
        }
    }
    let mut adjacency: Vec<Vec<(NodeId, u32)>> = vec![Vec::new(); kept.len()];
    for (ei, &(a, b)) in edges.iter().enumerate() {
        adjacency[a as usize].push((b, ei as u32));
        adjacency[b as usize].push((a, ei as u32));
    }

    let core = index_of(core_ax)? as NodeId;

    let map_stub = SupplyMap {
        seed: public_seed,
        axial: kept.clone(),
        edges: edges.clone(),
        adjacency: adjacency.clone(),
        core,
        wells: Vec::new(),
        rifts: Vec::new(),
    };
    let dist = map_stub.distances(core);

    // --- connectivity: entire kept graph must be reachable from core ---
    if dist.iter().any(|d| d.is_none()) {
        return None;
    }

    // --- rifts: east rim survivors, pairwise dist >= 3, core dist >= 6 ---
    let mut rift_candidates: Vec<NodeId> = east_rim
        .iter()
        .filter_map(|ax| index_of(*ax))
        .map(|i| i as NodeId)
        .filter(|&n| dist[n as usize].unwrap_or(0) >= 6)
        .collect();
    rift_candidates.shuffle(&mut rng);
    let mut rifts: Vec<NodeId> = Vec::new();
    for cand in rift_candidates {
        if rifts.len() >= spec.rifts {
            break;
        }
        let ok = rifts.iter().all(|&r| {
            let dr = map_stub.distances(r);
            dr[cand as usize].unwrap_or(0) >= 3
        });
        if ok {
            rifts.push(cand);
        }
    }
    if rifts.len() < spec.rifts {
        return None;
    }

    // --- wells: dist to core 2..=(radius*2), pairwise >= 2, not core/rift ---
    let max_d = (spec.radius * 2) as u32;
    let mut well_candidates: Vec<NodeId> = (0..kept.len() as NodeId)
        .filter(|&n| {
            let d = dist[n as usize].unwrap();
            (2..=max_d).contains(&d) && n != core && !rifts.contains(&n)
        })
        .collect();
    well_candidates.shuffle(&mut rng);
    let mut wells: Vec<NodeId> = Vec::new();
    for cand in well_candidates {
        if wells.len() >= spec.wells {
            break;
        }
        let ok = wells.iter().all(|&w| {
            let dw = map_stub.distances(w);
            dw[cand as usize].unwrap_or(0) >= 2
        });
        if ok {
            wells.push(cand);
        }
    }
    if wells.len() < spec.wells {
        return None;
    }

    Some(SupplyMap {
        wells,
        rifts,
        ..map_stub
    })
}
