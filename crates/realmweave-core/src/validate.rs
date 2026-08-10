//! Structural, symmetry, and fairness validation for board definitions.
//!
//! Fairness must be testable against the graph itself — never assumed from
//! visual symmetry. Every board file shipped in `boards/` must pass
//! `validate_board` in CI.

use std::collections::{HashMap, HashSet};

use crate::board::{BoardDefinition, BoardGraph, EdgeKind, NodeId, Player, Realm};
use crate::boardgen::{hex_distance, mirror, rotate60};

/// A board validation failure. Variant messages are self-describing.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[allow(missing_docs)] // each variant's #[error] text IS its documentation
pub enum ValidationError {
    #[error("board construction failed: {0}")]
    Graph(#[from] crate::board::BoardError),
    #[error("duplicate edge between {0} and {1}")]
    DuplicateEdge(NodeId, NodeId),
    #[error("self edge at node {0}")]
    SelfEdge(NodeId),
    #[error("isolated node {0}")]
    IsolatedNode(NodeId),
    #[error("intra-realm edge {0}-{1} crosses realms")]
    IntraRealmCrossesRealms(NodeId, NodeId),
    #[error("portal edge {0}-{1} does not link adjacent realms")]
    InvalidPortal(NodeId, NodeId),
    #[error("player {player:?} has {count} origins, expected one per realm")]
    BadOriginCount { player: Player, count: usize },
    #[error("player {player:?} is missing an origin in {realm:?}")]
    MissingOriginRealm { player: Player, realm: Realm },
    #[error("origin node {0} listed for both players")]
    SharedOrigin(NodeId),
    #[error("graph is not connected: {unreached} of {total} nodes unreachable")]
    Disconnected { unreached: usize, total: usize },
    #[error("realm sizes differ: {0:?}")]
    UnequalRealmSizes([usize; 3]),
    #[error("realms are not topologically equivalent under the id correspondence")]
    RealmsNotEquivalent,
    #[error("expected symmetry map is not a graph automorphism: {0}")]
    NotAutomorphism(&'static str),
    #[error("origin fairness violated: {0}")]
    Unfair(String),
}

/// Full validation; returns the constructed graph on success so callers can
/// reuse it.
pub fn validate_board(definition: &BoardDefinition) -> Result<BoardGraph, ValidationError> {
    let graph = BoardGraph::new(definition.clone())?;
    let def = graph.definition();

    // --- structural checks ---
    let mut edge_keys = HashSet::new();
    for edge in &def.edges {
        if edge.a == edge.b {
            return Err(ValidationError::SelfEdge(edge.a));
        }
        if !edge_keys.insert(edge.key()) {
            return Err(ValidationError::DuplicateEdge(edge.a, edge.b));
        }
        // Merged-field boards (triforce): realm tags are interior regions
        // of ONE battlefield — edges legitimately cross region boundaries.
        if !def.id.starts_with("tf") {
            let (ra, rb) = (graph.realm_of(edge.a), graph.realm_of(edge.b));
            match edge.kind {
                EdgeKind::IntraRealm => {
                    if ra != rb {
                        return Err(ValidationError::IntraRealmCrossesRealms(edge.a, edge.b));
                    }
                }
                EdgeKind::Portal => {
                    if !ra.is_adjacent(rb) {
                        return Err(ValidationError::InvalidPortal(edge.a, edge.b));
                    }
                }
            }
        }
    }
    for node in &def.nodes {
        if graph.neighbors(node.id).is_empty() {
            return Err(ValidationError::IsolatedNode(node.id));
        }
    }

    // --- origins ---
    // Side-goal boards (trinity: realm sides are the goals) legitimately
    // have zero origins; realms are then intentionally disconnected from
    // each other, so per-realm connectivity is checked instead.
    let side_goal = def.origins.is_empty();
    let mut origin_nodes = HashSet::new();
    for origin in &def.origins {
        if !origin_nodes.insert(origin.node) {
            return Err(ValidationError::SharedOrigin(origin.node));
        }
    }
    if !side_goal {
        for player in [Player::Light, Player::Dark] {
            let origins = def.origins_of(player);
            if origins.len() != 3 {
                return Err(ValidationError::BadOriginCount {
                    player,
                    count: origins.len(),
                });
            }
            for realm in Realm::ALL {
                if !origins.iter().any(|&n| graph.realm_of(n) == realm) {
                    return Err(ValidationError::MissingOriginRealm { player, realm });
                }
            }
        }
    }

    // --- connectivity ---
    let merged_field = def.id.starts_with("tf");
    if merged_field {
        // One connected battlefield.
        let dist = graph.bfs_distances(0);
        let unreached = dist.iter().filter(|d| d.is_none()).count();
        if unreached > 0 {
            return Err(ValidationError::Disconnected {
                unreached,
                total: graph.node_count(),
            });
        }
    } else if side_goal {
        // Each realm must be internally connected.
        let per_realm = graph.node_count() / 3;
        for realm in 0..3 {
            let start = (realm * per_realm) as NodeId;
            let dist = graph.bfs_distances(start);
            let lo = realm * per_realm;
            let hi = lo + per_realm;
            let unreached = (lo..hi).filter(|&i| dist[i].is_none()).count();
            if unreached > 0 {
                return Err(ValidationError::Disconnected {
                    unreached,
                    total: per_realm,
                });
            }
        }
    } else {
        let dist = graph.bfs_distances(0);
        let unreached = dist.iter().filter(|d| d.is_none()).count();
        if unreached > 0 {
            return Err(ValidationError::Disconnected {
                unreached,
                total: graph.node_count(),
            });
        }
    }

    // --- realm equivalence (not applicable to merged-field boards where
    // realm tags are interior regions of one triangle) ---
    if !merged_field {
        check_realm_equivalence(&graph)?;
    }

    // --- symmetry automorphisms (hex boards with axial data) ---
    check_hex_symmetry(&graph)?;

    // --- origin fairness (origin boards only) ---
    if !side_goal {
        check_origin_fairness(&graph)?;
    }

    Ok(graph)
}

/// Realms must be pairwise isomorphic. When node ids are realm-major with an
/// identical per-realm ordering (`id % realm_size` correspondence), the check
/// is exact; the correspondence is derived from sorted per-realm id lists.
fn check_realm_equivalence(graph: &BoardGraph) -> Result<(), ValidationError> {
    let def = graph.definition();
    let mut by_realm: [Vec<NodeId>; 3] = Default::default();
    for node in &def.nodes {
        by_realm[node.realm.index()].push(node.id);
    }
    for ids in &mut by_realm {
        ids.sort_unstable();
    }
    let sizes = [by_realm[0].len(), by_realm[1].len(), by_realm[2].len()];
    if sizes[0] != sizes[1] || sizes[1] != sizes[2] {
        return Err(ValidationError::UnequalRealmSizes(sizes));
    }

    // Local index within realm, by id order.
    let mut local: HashMap<NodeId, usize> = HashMap::new();
    for ids in &by_realm {
        for (i, &id) in ids.iter().enumerate() {
            local.insert(id, i);
        }
    }
    let mut edge_sets: [HashSet<(usize, usize)>; 3] = Default::default();
    for edge in &def.edges {
        if edge.kind != EdgeKind::IntraRealm {
            continue;
        }
        let realm = graph.realm_of(edge.a);
        let (mut a, mut b) = (local[&edge.a], local[&edge.b]);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        edge_sets[realm.index()].insert((a, b));
    }
    if edge_sets[0] != edge_sets[1] || edge_sets[1] != edge_sets[2] {
        return Err(ValidationError::RealmsNotEquivalent);
    }
    Ok(())
}

/// For hex boards (all nodes carry axial coordinates), verify that 60°
/// rotation and axis mirror — applied per realm — are graph automorphisms.
fn check_hex_symmetry(graph: &BoardGraph) -> Result<(), ValidationError> {
    let def = graph.definition();
    if def.nodes.iter().any(|n| n.axial.is_none()) {
        return Ok(()); // non-hex boards skip exact symmetry validation
    }
    // Triangle boards (side-goal, "triN-" ids) carry (row, col) coords, not
    // hex axials: their symmetry group is the triangle's S3. Validate the
    // left-right mirror (col -> row - col), which any fair triangle must
    // admit; the 120° rotations need barycentric coords and are guaranteed
    // by construction in the generator.
    if def.id.starts_with("tf") {
        // Triforce: one triangle; realm tags mark interior regions, not
        // separate components. Mirror symmetry (c → r−c) must hold.
        let index = graph.axial_index();
        let mut map = vec![0; graph.node_count()];
        for node in &def.nodes {
            let Some([r, c]) = node.axial else {
                return Ok(());
            };
            let Some(target) = index.get(&(node.realm, [r, r - c])).or_else(|| {
                // mirrored node may carry a different realm tag (left/right
                // swap): search all realms at that coordinate
                crate::board::Realm::ALL
                    .iter()
                    .find_map(|&rm| index.get(&(rm, [r, r - c])))
            }) else {
                return Err(ValidationError::NotAutomorphism("triforce mirror"));
            };
            map[node.id as usize] = *target;
        }
        if !is_automorphism(graph, &map) {
            return Err(ValidationError::NotAutomorphism("triforce mirror"));
        }
        return Ok(());
    }
    if def.id.starts_with("tri") {
        let index = graph.axial_index();
        let mut map = vec![0; graph.node_count()];
        for node in &def.nodes {
            let Some([r, c]) = node.axial else {
                return Ok(());
            };
            let Some(target) = index.get(&(node.realm, [r, r - c])) else {
                return Err(ValidationError::NotAutomorphism("triangle mirror"));
            };
            map[node.id as usize] = *target;
        }
        if !is_automorphism(graph, &map) {
            return Err(ValidationError::NotAutomorphism("triangle mirror"));
        }
        return Ok(());
    }
    let index = graph.axial_index();
    let map_with = |f: fn([i32; 2]) -> [i32; 2]| -> Option<Vec<NodeId>> {
        let mut map = vec![0; graph.node_count()];
        for node in &def.nodes {
            let target = index.get(&(node.realm, f(node.axial?)))?;
            map[node.id as usize] = *target;
        }
        Some(map)
    };
    for (name, f) in [
        ("60-degree rotation", rotate60 as fn([i32; 2]) -> [i32; 2]),
        ("axis mirror", mirror as fn([i32; 2]) -> [i32; 2]),
    ] {
        let Some(map) = map_with(f) else {
            return Err(ValidationError::NotAutomorphism(name));
        };
        if !is_automorphism(graph, &map) {
            return Err(ValidationError::NotAutomorphism(name));
        }
    }
    Ok(())
}

/// A node permutation is an automorphism iff it maps the edge set onto itself
/// preserving edge kinds.
pub fn is_automorphism(graph: &BoardGraph, map: &[NodeId]) -> bool {
    let def = graph.definition();
    let mut edges: HashSet<(NodeId, NodeId, EdgeKind)> = HashSet::new();
    for e in &def.edges {
        let (a, b) = e.key();
        edges.insert((a, b, e.kind));
    }
    def.edges.iter().all(|e| {
        let mapped = crate::board::Edge {
            a: map[e.a as usize],
            b: map[e.b as usize],
            kind: e.kind,
        };
        let (a, b) = mapped.key();
        edges.contains(&(a, b, e.kind))
    })
}

/// Both players must see structurally equivalent starting conditions:
/// identical sorted multisets of (a) pairwise own-origin distances and
/// (b) distances from each origin to the gate set.
fn check_origin_fairness(graph: &BoardGraph) -> Result<(), ValidationError> {
    let def = graph.definition();
    let gates = def.gate_nodes();
    let profile = |player: Player| -> (Vec<u32>, Vec<u32>) {
        let origins = def.origins_of(player);
        let mut pairwise = Vec::new();
        let mut to_gates = Vec::new();
        for &o in &origins {
            let dist = graph.bfs_distances(o);
            for &other in &origins {
                if other > o {
                    pairwise.push(dist[other as usize].unwrap_or(u32::MAX));
                }
            }
            let mut gate_dists: Vec<u32> = gates
                .iter()
                .map(|&g| dist[g as usize].unwrap_or(u32::MAX))
                .collect();
            gate_dists.sort_unstable();
            to_gates.extend(gate_dists);
        }
        pairwise.sort_unstable();
        to_gates.sort_unstable();
        (pairwise, to_gates)
    };
    let light = profile(Player::Light);
    let dark = profile(Player::Dark);
    if light.0 != dark.0 {
        return Err(ValidationError::Unfair(format!(
            "pairwise origin distances differ: Light {:?} vs Dark {:?}",
            light.0, dark.0
        )));
    }
    if light.1 != dark.1 {
        return Err(ValidationError::Unfair(
            "origin-to-gate distance profiles differ".to_string(),
        ));
    }
    Ok(())
}

/// Convenience metrics used by fairness tooling and tests.
pub fn degree_histogram(graph: &BoardGraph) -> HashMap<usize, usize> {
    let mut hist = HashMap::new();
    for node in &graph.definition().nodes {
        *hist.entry(graph.neighbors(node.id).len()).or_insert(0) += 1;
    }
    hist
}

/// Hex ring index of a node (requires axial data).
pub fn ring_of(node_axial: [i32; 2]) -> i32 {
    hex_distance(node_axial, [0, 0])
}
