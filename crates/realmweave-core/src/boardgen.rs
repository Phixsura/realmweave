//! Deterministic generator for the sixfold-symmetric hex family of boards.
//!
//! Board sizes are centered hexagonal numbers: radius 2 → 19 nodes/realm,
//! radius 3 → 37, radius 4 → 61. Three stacked realms share the same axial
//! topology; cross-realm connectivity happens only at configurable gates.
//!
//! Layout conventions (all deterministic):
//! - Axial coordinates `(q, r)` with pointy-top hexes, ring(k) = hex distance k.
//! - Node ids are realm-major: Heaven block, then Mortal, then Underworld,
//!   each block ordered identically, so `id % realm_size` is the cross-realm
//!   correspondence used by symmetry validation.
//! - Gates: "inner" gates are the six ring-1 nodes, "outer" gates the six
//!   corners of ring (radius-1). On the radius-2 board those coincide, so the
//!   19-board has 6 gates. Each gate column links Heaven↔Mortal and
//!   Mortal↔Underworld (adjacent realms only) at the same axial coordinate.
//! - Origins: each player owns one outer-ring corner per realm, rotated 120°
//!   across realms; Dark is Light's 180° point reflection. Origins never sit
//!   on gate nodes.

use serde::{Deserialize, Serialize};

use crate::board::{BoardDefinition, Edge, EdgeKind, Node, NodeId, Origin, Player, Realm};

/// Axial hex directions, counter-clockwise starting East.
pub const HEX_DIRS: [[i32; 2]; 6] = [[1, 0], [0, 1], [-1, 1], [-1, 0], [0, -1], [1, -1]];

/// Vertical distance between realm layers in the default layout hint.
pub const LAYER_HEIGHT: f32 = 4.0;

/// Which axial coordinates carry portal columns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortalSpec {
    /// Six ring-1 gates plus the six corners of ring (radius - 1).
    /// The two sets coincide on the radius-2 board.
    Inner6Outer6,
    /// Explicit axial gate coordinates (experimental topologies).
    Explicit(Vec<[i32; 2]>),
}

impl PortalSpec {
    /// Axial coordinates of the gate columns for a board of `radius`.
    pub fn gate_axials(&self, radius: i32) -> Vec<[i32; 2]> {
        match self {
            PortalSpec::Inner6Outer6 => {
                let mut gates: Vec<[i32; 2]> = HEX_DIRS.iter().map(|d| [d[0], d[1]]).collect();
                let outer = radius - 1;
                for d in HEX_DIRS {
                    let corner = [d[0] * outer, d[1] * outer];
                    if !gates.contains(&corner) {
                        gates.push(corner);
                    }
                }
                gates
            }
            PortalSpec::Explicit(axials) => axials.clone(),
        }
    }

    /// Short human-readable name of this portal layout.
    pub fn label(&self) -> &'static str {
        match self {
            PortalSpec::Inner6Outer6 => "inner6-outer6",
            PortalSpec::Explicit(_) => "explicit",
        }
    }
}

/// Generation parameters for a hexagonal three-realm board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HexBoardSpec {
    /// Hex radius; nodes per realm = 1 + 3*radius*(radius+1).
    pub radius: i32,
    /// Gate column layout.
    pub portals: PortalSpec,
}

impl HexBoardSpec {
    /// Standard spec for a supported realm size (19/37/61/91/127).
    pub fn from_realm_size(size: usize) -> Option<Self> {
        let radius = match size {
            19 => 2,
            37 => 3,
            61 => 4,
            91 => 5,
            127 => 6,
            _ => return None,
        };
        Some(HexBoardSpec {
            radius,
            portals: PortalSpec::Inner6Outer6,
        })
    }

    /// Nodes per realm for this radius.
    pub fn realm_size(&self) -> usize {
        let r = self.radius as usize;
        1 + 3 * r * (r + 1)
    }
}

/// Hex grid distance between two axial coordinates.
pub fn hex_distance(a: [i32; 2], b: [i32; 2]) -> i32 {
    let dq = a[0] - b[0];
    let dr = a[1] - b[1];
    (dq.abs() + dr.abs() + (dq + dr).abs()) / 2
}

/// Rotate an axial coordinate 60° counter-clockwise about the center.
pub fn rotate60(ax: [i32; 2]) -> [i32; 2] {
    [-ax[1], ax[0] + ax[1]]
}

/// Reflect an axial coordinate across the q-axis.
pub fn mirror(ax: [i32; 2]) -> [i32; 2] {
    [ax[0] + ax[1], -ax[1]]
}

/// All axial coordinates within `radius`, in deterministic order.
pub fn hex_coords(radius: i32) -> Vec<[i32; 2]> {
    let mut coords = Vec::new();
    for q in -radius..=radius {
        for r in -radius..=radius {
            if hex_distance([q, r], [0, 0]) <= radius {
                coords.push([q, r]);
            }
        }
    }
    coords
}

fn axial_to_xz(ax: [i32; 2]) -> (f32, f32) {
    let q = ax[0] as f32;
    let r = ax[1] as f32;
    let x = 3f32.sqrt() * (q + r / 2.0);
    let z = 1.5 * r;
    (x, z)
}

fn realm_y(realm: Realm) -> f32 {
    match realm {
        Realm::Heaven => LAYER_HEIGHT,
        Realm::Mortal => 0.0,
        Realm::Underworld => -LAYER_HEIGHT,
    }
}

/// Corner of the outer ring in hex direction `dir_index`.
fn outer_corner(radius: i32, dir_index: usize) -> [i32; 2] {
    let d = HEX_DIRS[dir_index % 6];
    [d[0] * radius, d[1] * radius]
}

/// Origin axial coordinates per realm for a player.
///
/// Light occupies outer corners in directions 0/2/4 for
/// Heaven/Mortal/Underworld; Dark the 180°-reflected pattern (3/5/1).
pub fn origin_axials(radius: i32, player: Player) -> [(Realm, [i32; 2]); 3] {
    let base = match player {
        Player::Light => 0,
        Player::Dark => 3,
    };
    [
        (Realm::Heaven, outer_corner(radius, base)),
        (Realm::Mortal, outer_corner(radius, base + 2)),
        (Realm::Underworld, outer_corner(radius, base + 4)),
    ]
}

/// Generate a full three-realm board definition.
pub fn generate(spec: &HexBoardSpec, id: &str, version: u32) -> BoardDefinition {
    let radius = spec.radius;
    let coords = hex_coords(radius);
    let realm_size = coords.len();
    debug_assert_eq!(realm_size, spec.realm_size());

    let index_of = |ax: [i32; 2]| -> usize {
        coords
            .iter()
            .position(|c| *c == ax)
            .unwrap_or_else(|| unreachable!("generator emits only in-radius axials"))
    };
    let node_id = |realm: Realm, ax: [i32; 2]| -> NodeId {
        (realm.index() * realm_size + index_of(ax)) as NodeId
    };

    let mut nodes = Vec::with_capacity(realm_size * 3);
    for realm in Realm::ALL {
        for &ax in &coords {
            let (x, z) = axial_to_xz(ax);
            nodes.push(Node {
                id: node_id(realm, ax),
                realm,
                position: [x, realm_y(realm), z],
                axial: Some(ax),
            });
        }
    }

    let mut edges = Vec::new();
    // Intra-realm hex adjacency, each undirected edge emitted once.
    for realm in Realm::ALL {
        for &ax in &coords {
            for d in HEX_DIRS {
                let nb = [ax[0] + d[0], ax[1] + d[1]];
                if hex_distance(nb, [0, 0]) <= radius && (ax[0], ax[1]) < (nb[0], nb[1]) {
                    edges.push(Edge {
                        a: node_id(realm, ax),
                        b: node_id(realm, nb),
                        kind: EdgeKind::IntraRealm,
                    });
                }
            }
        }
    }
    // Portal columns: adjacent realms only.
    for ax in spec.portals.gate_axials(radius) {
        assert!(
            hex_distance(ax, [0, 0]) <= radius,
            "gate axial {ax:?} outside radius {radius}"
        );
        edges.push(Edge {
            a: node_id(Realm::Heaven, ax),
            b: node_id(Realm::Mortal, ax),
            kind: EdgeKind::Portal,
        });
        edges.push(Edge {
            a: node_id(Realm::Mortal, ax),
            b: node_id(Realm::Underworld, ax),
            kind: EdgeKind::Portal,
        });
    }

    let mut origins = Vec::new();
    for player in [Player::Light, Player::Dark] {
        for (realm, ax) in origin_axials(radius, player) {
            origins.push(Origin {
                player,
                node: node_id(realm, ax),
            });
        }
    }

    BoardDefinition {
        id: id.to_string(),
        version,
        nodes,
        edges,
        origins,
    }
}

/// Generate the standard board for a realm size (19, 37, or 61 nodes/realm).
pub fn generate_standard(realm_size: usize) -> Option<BoardDefinition> {
    let spec = HexBoardSpec::from_realm_size(realm_size)?;
    let id = format!("hex{realm_size}-v1");
    Some(generate(&spec, &id, 1))
}

/// Seeded board variant: start from the standard hex disc and carve
/// symmetric pairs of holes (point-reflected, so fairness is preserved by
/// construction). Every seed is a different world; the validator still
/// runs downstream, and generation rerolls until it passes.
///
/// Carved nodes never include origins, gates, or their neighbors.
pub fn generate_seeded(realm_size: usize, seed: u64) -> Option<BoardDefinition> {
    // Gate family also varies per seed (rotation-closed subsets keep the
    // symmetry validator happy). Together with orbit carving this gives
    // dozens of distinct worlds per size; radius >= 4 recommended (radius 3
    // has only one carvable orbit).
    let base_spec = HexBoardSpec::from_realm_size(realm_size)?;
    for attempt in 0u64..64 {
        let sub = seed.wrapping_add(attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        // pick gate family from seed
        let fam = (seed.wrapping_add(attempt) % 3) as u8;
        let portals = match fam {
            0 => PortalSpec::Inner6Outer6,
            1 => PortalSpec::Explicit(HEX_DIRS.iter().map(|d| [d[0], d[1]]).collect()),
            _ => {
                let outer = base_spec.radius - 1;
                PortalSpec::Explicit(
                    HEX_DIRS
                        .iter()
                        .map(|d| [d[0] * outer, d[1] * outer])
                        .collect(),
                )
            }
        };
        let spec = HexBoardSpec {
            radius: base_spec.radius,
            portals,
        };
        if let Some(def) = try_seeded(&spec, realm_size, seed, sub) {
            return Some(def);
        }
    }
    // Uncarvable seed: fall back to the standard board, but the id MUST
    // stay the requested seeded id — resolve("hexN-sSEED") returning a
    // definition labeled "hexN-v1" makes every stored game on that seed
    // fail Game::new's BoardMismatch forever.
    let mut def = generate_standard(realm_size)?;
    def.id = format!("hex{realm_size}-s{seed}");
    Some(def)
}

fn try_seeded(
    spec: &HexBoardSpec,
    realm_size: usize,
    public_seed: u64,
    sub_seed: u64,
) -> Option<BoardDefinition> {
    let base = generate(spec, &format!("hex{realm_size}-s{public_seed}"), 1);
    // Simple xorshift for determinism without a rand dependency here.
    let mut state = sub_seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    // Protected axials: origins, gates, and their ring-1 neighbors.
    let gate_axials = spec.portals.gate_axials(spec.radius);
    let mut protected: Vec<[i32; 2]> = gate_axials.clone();
    for player in [Player::Light, Player::Dark] {
        for (_, ax) in origin_axials(spec.radius, player) {
            protected.push(ax);
            // 2-hop protection: carving near origins creates sealable
            // pockets that make stone-wall strangles trivial.
            for d in HEX_DIRS {
                let n1 = [ax[0] + d[0], ax[1] + d[1]];
                protected.push(n1);
                for d2 in HEX_DIRS {
                    protected.push([n1[0] + d2[0], n1[1] + d2[1]]);
                }
            }
        }
    }

    // Candidate axials for carving: interior, unprotected, not center.
    let candidates: Vec<[i32; 2]> = hex_coords(spec.radius)
        .into_iter()
        .filter(|ax| {
            *ax != [0, 0] && !protected.contains(ax) && hex_distance(*ax, [0, 0]) < spec.radius
            // keep the rim
        })
        .collect();

    // Carve whole ROTATION ORBITS (all six 60°-rotations of one axial):
    // the validator demands the rot-60 automorphism, and an orbit is the
    // only carve unit that preserves it. Each orbit = 6 holes per realm.
    // 1-2 orbits per seed; WHICH orbit(s) is the seed's fingerprint.
    let mut orbit_reps: Vec<[i32; 2]> = Vec::new();
    for ax in &candidates {
        // canonical representative: lexicographically smallest in its orbit
        let mut orbit = vec![*ax];
        let mut cur = *ax;
        for _ in 0..5 {
            cur = rotate60(cur);
            orbit.push(cur);
        }
        // orbit must be fully inside candidates (no protected/rim overlap)
        if !orbit.iter().all(|o| candidates.contains(o)) {
            continue;
        }
        let Some(&rep) = orbit.iter().min() else {
            continue; // orbits are non-empty by construction
        };
        if !orbit_reps.contains(&rep) {
            orbit_reps.push(rep);
        }
    }
    if orbit_reps.is_empty() {
        return None;
    }
    let orbits_to_carve = 1usize; // 2 orbits over-thins routes → strangle-heavy games
    let mut carved: Vec<[i32; 2]> = Vec::new();
    let mut chosen: Vec<[i32; 2]> = Vec::new();
    let mut guard = 0;
    while chosen.len() < orbits_to_carve && guard < 100 {
        guard += 1;
        let rep = orbit_reps[(next() % orbit_reps.len() as u64) as usize];
        if chosen.contains(&rep) {
            continue;
        }
        chosen.push(rep);
        let mut cur = rep;
        carved.push(cur);
        for _ in 0..5 {
            cur = rotate60(cur);
            if !carved.contains(&cur) {
                carved.push(cur);
            }
        }
    }
    if carved.is_empty() {
        return None;
    }

    // Remove carved nodes (in every realm) and re-index densely.
    let keep: Vec<&Node> = base
        .nodes
        .iter()
        .filter(|n| n.axial.is_none_or(|ax| !carved.contains(&ax)))
        .collect();
    let mut remap = std::collections::HashMap::new();
    let mut nodes = Vec::with_capacity(keep.len());
    for (new_id, n) in keep.iter().enumerate() {
        remap.insert(n.id, new_id as NodeId);
        let mut node = (*n).clone();
        node.id = new_id as NodeId;
        nodes.push(node);
    }
    let edges: Vec<Edge> = base
        .edges
        .iter()
        .filter_map(|e| {
            let a = *remap.get(&e.a)?;
            let b = *remap.get(&e.b)?;
            Some(Edge { a, b, kind: e.kind })
        })
        .collect();
    let origins: Vec<Origin> = base
        .origins
        .iter()
        .map(|o| Origin {
            player: o.player,
            node: *remap.get(&o.node).unwrap_or(&o.node), // origins never carved
        })
        .collect();

    let def = BoardDefinition {
        id: format!("hex{realm_size}-s{public_seed}"),
        version: 1,
        nodes,
        edges,
        origins,
    };
    // Downstream validation gate: connectivity + realm equivalence +
    // origin fairness must all hold, else reroll.
    crate::validate::validate_board(&def).ok()?;
    Some(def)
}

// ------------------------------------------------------------- trinity-y ---

/// v4 "Trinity Y" board: three TRIANGULAR realms. In each realm the goal is
/// the game of Y — connect all three sides with one group. The Y theorem
/// guarantees a full triangle has exactly one Y-winner, so every realm is
/// decisive and the match (best of three realms) can never be drawn.
///
/// Triangle coordinates: rows 0..side, row r has r+1 cells; realm-major ids.
/// Sides: 0 = left (c==0), 1 = right (c==r), 2 = bottom (r==side-1).
/// No origins (goals are the sides themselves), no portals in v4.0 —
/// coupling between realms is pure tempo: one stone per turn, any realm.
pub fn generate_trinity(side: usize) -> Option<BoardDefinition> {
    if !(4..=26).contains(&side) {
        return None;
    }
    let per_realm = side * (side + 1) / 2;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let index = |realm: usize, r: usize, c: usize| -> NodeId {
        (realm * per_realm + r * (r + 1) / 2 + c) as NodeId
    };
    for (ri, realm) in Realm::ALL.iter().enumerate() {
        let y = realm_y(*realm);
        for r in 0..side {
            for c in 0..=r {
                // equilateral layout: row r spans width r
                let x = c as f32 - r as f32 / 2.0;
                let z = r as f32 * 0.866;
                nodes.push(Node {
                    id: index(ri, r, c),
                    realm: *realm,
                    position: [x, y, z - side as f32 * 0.433],
                    axial: Some([r as i32, c as i32]),
                });
            }
        }
        for r in 0..side {
            for c in 0..=r {
                let a = index(ri, r, c);
                // right neighbor, and the two "children" below
                if c < r {
                    edges.push(Edge {
                        a,
                        b: index(ri, r, c + 1),
                        kind: EdgeKind::IntraRealm,
                    });
                }
                if r + 1 < side {
                    edges.push(Edge {
                        a,
                        b: index(ri, r + 1, c),
                        kind: EdgeKind::IntraRealm,
                    });
                    edges.push(Edge {
                        a,
                        b: index(ri, r + 1, c + 1),
                        kind: EdgeKind::IntraRealm,
                    });
                }
            }
        }
    }
    Some(BoardDefinition {
        id: format!("tri{side}-v4"),
        version: 1,
        nodes,
        edges,
        origins: Vec::new(),
    })
}

/// Which sides of its triangular realm a trinity node lies on (bitmask:
/// 1 = left, 2 = right, 4 = bottom). Zero for interior nodes.
pub fn trinity_sides(side: usize, node: NodeId) -> u8 {
    let per_realm = side * (side + 1) / 2;
    let local = node as usize % per_realm;
    // invert triangular number: r = floor((sqrt(8*local+1)-1)/2)
    let r = ((((8 * local + 1) as f64).sqrt() - 1.0) / 2.0) as usize;
    let c = local - r * (r + 1) / 2;
    let mut mask = 0u8;
    if c == 0 {
        mask |= 1;
    }
    if c == r {
        mask |= 2;
    }
    if r == side - 1 {
        mask |= 4;
    }
    mask
}

/// Resolve a board id to its definition, regenerating deterministically for
/// every generated family: `hex{19,37,61,91,127}-v1` and `tri{4..26}-v4`.
/// The single source of truth for id → board across replay, resume, CLI,
/// and tooling — file lookups are the caller's fallback, not the default.
pub fn resolve(board_id: &str) -> Option<BoardDefinition> {
    if let Some(rest) = board_id.strip_prefix("hex") {
        if let Some(size) = rest.strip_suffix("-v1").and_then(|s| s.parse().ok()) {
            return generate_standard(size);
        }
        // Seeded worlds: hex{size}-s{seed} regenerates deterministically.
        if let Some((size, seed)) = rest.split_once("-s") {
            if let (Ok(size), Ok(seed)) = (size.parse(), seed.parse()) {
                return generate_seeded(size, seed);
            }
        }
    }
    if let Some(rest) = board_id.strip_prefix("tri") {
        if let Some(side) = rest.strip_suffix("-v4").and_then(|s| s.parse().ok()) {
            return generate_trinity(side);
        }
    }
    if let Some(rest) = board_id.strip_prefix("tf") {
        if let Some(side) = rest.strip_suffix("-v5p").and_then(|s| s.parse().ok()) {
            return generate_triforce_pierced(side);
        }
        if let Some(side) = rest.strip_suffix("-v5").and_then(|s| s.parse().ok()) {
            return generate_triforce(side);
        }
    }
    None
}

// -------------------------------------------------------------- triforce ---

/// v5 "Triforce" board: ONE large triangle (side length `side`) whose three
/// corner sub-triangles are the realms — Heaven (top), Mortal (left),
/// Underworld (right) — and whose central inverted triangle is the
/// weave-heart where all three realms meet. One connected battlefield:
/// winning paths must cross realms (measured: 100% of random-fill wins
/// span ≥2 realms and touch the heart; docs/research-triforce.md).
///
/// Coordinates are (row, col) like trinity boards. `side` must be even so
/// the four sub-triangles are exact. Node realm tags mark the corner
/// regions for rendering; heart nodes are tagged `Mortal` (tags are visual
/// metadata here — legality only reads the graph).
pub fn generate_triforce(side: usize) -> Option<BoardDefinition> {
    if !(8..=40).contains(&side) || !side.is_multiple_of(2) {
        return None;
    }
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let index = |r: usize, c: usize| -> NodeId { (r * (r + 1) / 2 + c) as NodeId };
    for r in 0..side {
        for c in 0..=r {
            let id = index(r, c);
            let realm = match tf_region_rc(side, r, c) {
                0 => Realm::Heaven,
                1 => Realm::Mortal,
                2 => Realm::Underworld,
                _ => Realm::Mortal, // heart: tag is cosmetic, region fn is truth
            };
            let x = c as f32 - r as f32 / 2.0;
            let z = r as f32 * 0.866 - side as f32 * 0.433;
            nodes.push(Node {
                id,
                realm,
                position: [x, 0.0, z],
                axial: Some([r as i32, c as i32]),
            });
            if c < r {
                edges.push(Edge {
                    a: id,
                    b: index(r, c + 1),
                    kind: EdgeKind::IntraRealm,
                });
            }
            if r + 1 < side {
                edges.push(Edge {
                    a: id,
                    b: index(r + 1, c),
                    kind: EdgeKind::IntraRealm,
                });
                edges.push(Edge {
                    a: id,
                    b: index(r + 1, c + 1),
                    kind: EdgeKind::IntraRealm,
                });
            }
        }
    }
    Some(BoardDefinition {
        id: format!("tf{side}-v5"),
        version: 1,
        nodes,
        edges,
        origins: Vec::new(),
    })
}

/// Big-triangle side length of a merged-triangle board: the deepest axial
/// row + 1. Works for solid (v5) AND pierced (v5p) boards, where the node
/// count no longer matches the triangular-number formula. Compute once per
/// scope; O(n).
pub fn tf_side_len(def: &BoardDefinition) -> usize {
    def.nodes
        .iter()
        .filter_map(|n| n.axial)
        .map(|a| a[0] as usize)
        .max()
        .map_or(0, |m| m + 1)
}

/// Region from a (row, col) coordinate: 0 = Heaven (top corner),
/// 1 = Mortal (left), 2 = Underworld (right), 3 = the weave-heart.
fn tf_region_rc(side: usize, r: usize, c: usize) -> usize {
    let h = side / 2;
    if r < h {
        return 0;
    }
    let rr = r - h;
    if c <= rr {
        return 1;
    }
    if c >= h {
        return 2;
    }
    3
}

/// Region of a triforce node, read from its axial coordinate — the single
/// authority for solid and pierced boards alike (`side` from
/// [`tf_side_len`]): 0 = Heaven, 1 = Mortal, 2 = Underworld, 3 = heart.
pub fn triforce_region(def: &BoardDefinition, side: usize, node: NodeId) -> usize {
    let [r, c] = def.nodes[node as usize].axial.unwrap_or([0, 0]);
    tf_region_rc(side, r as usize, c as usize)
}

/// Which sides of the BIG triangle a triforce node lies on (bitmask:
/// 1 = left, 2 = right, 4 = bottom). Same convention as `trinity_sides`;
/// axial-based like [`triforce_region`].
pub fn triforce_sides(def: &BoardDefinition, side: usize, node: NodeId) -> u8 {
    let [r, c] = def.nodes[node as usize].axial.unwrap_or([0, 0]);
    let (r, c) = (r as usize, c as usize);
    let mut mask = 0u8;
    if c == 0 {
        mask |= 1;
    }
    if c == r {
        mask |= 2;
    }
    if r == side - 1 {
        mask |= 4;
    }
    mask
}

// ------------------------------------------------------------- pierced ---

/// Hexagonal lattice distance between two (row, col) triangle coordinates.
fn tri_dist(a: (usize, usize), b: (usize, usize)) -> usize {
    let (a1, b1) = (a.0 as i64 - a.1 as i64, a.1 as i64);
    let (a2, b2) = (b.0 as i64 - b.1 as i64, b.1 as i64);
    let (da, db) = (a1 - a2, b1 - b2);
    ((da.abs() + db.abs() + (da + db).abs()) / 2) as usize
}

/// The canonical S3-symmetric deletion orbit for a pierced board: six
/// deep-interior heart nodes closed under the triangle's full symmetry
/// group, pairwise lattice distance >= 2 (holes never share a ring node's
/// edge), chosen deterministically (max min-distance, then lexicographic).
/// None when the heart is too small to host one (side < 22).
fn pierced_orbit(side: usize) -> Option<[(usize, usize); 6]> {
    let in_tri = |r: i64, c: i64| r >= 0 && (r as usize) < side && c >= 0 && c <= r;
    let neigh = |r: usize, c: usize| -> [(i64, i64); 6] {
        let (r, c) = (r as i64, c as i64);
        [
            (r, c - 1),
            (r, c + 1),
            (r - 1, c - 1),
            (r - 1, c),
            (r + 1, c),
            (r + 1, c + 1),
        ]
    };
    // Deep-interior heart nodes: heart-region cells all of whose lattice
    // neighbors exist and are heart cells too.
    let deep: Vec<(usize, usize)> = (0..side)
        .flat_map(|r| (0..=r).map(move |c| (r, c)))
        .filter(|&(r, c)| {
            tf_region_rc(side, r, c) == 3
                && neigh(r, c).iter().all(|&(nr, nc)| {
                    in_tri(nr, nc) && tf_region_rc(side, nr as usize, nc as usize) == 3
                })
        })
        .collect();
    // Full S3 orbit of a cell via barycentric (u, v, w) permutations.
    let orbit_of = |(r, c): (usize, usize)| -> Vec<(usize, usize)> {
        let (u, v, w) = (side - 1 - r, c, r - c);
        let mut pts: Vec<(usize, usize)> = [
            (u, v, w),
            (u, w, v),
            (v, u, w),
            (v, w, u),
            (w, u, v),
            (w, v, u),
        ]
        .iter()
        .map(|&(a, b, _)| (side - 1 - a, b))
        .collect();
        pts.sort_unstable();
        pts.dedup();
        pts
    };
    let mut best: Option<(usize, Vec<(usize, usize)>)> = None;
    for &p in &deep {
        let orbit = orbit_of(p);
        if orbit.len() != 6 || !orbit.iter().all(|q| deep.contains(q)) {
            continue;
        }
        let mut min_d = usize::MAX;
        for i in 0..6 {
            for j in i + 1..6 {
                min_d = min_d.min(tri_dist(orbit[i], orbit[j]));
            }
        }
        if min_d < 2 {
            continue;
        }
        let better = match &best {
            None => true,
            Some((bd, bo)) => min_d > *bd || (min_d == *bd && orbit < *bo),
        };
        if better {
            best = Some((min_d, orbit));
        }
    }
    best.map(|(_, o)| {
        let mut arr = [(0, 0); 6];
        arr.copy_from_slice(&o);
        arr
    })
}

/// v5p "pierced Triforce": the v5 merged triangle with six weave-heart
/// nodes removed (the canonical S3-symmetric orbit) and each hexagonal
/// hole re-triangulated by a fan from its outermost ring node. This is the
/// documented counter-measure to flat-Y center dominance: fewer heart
/// nodes, lower connectivity through the middle, every internal face still
/// a triangle so the Y theorem (no draws) is untouched. Sides: even
/// 22..=40 (smaller hearts cannot host the orbit). Id: `tf{side}-v5p`.
#[allow(clippy::expect_used)] // generator-internal invariants; tests cover every side
pub fn generate_triforce_pierced(side: usize) -> Option<BoardDefinition> {
    if !(22..=40).contains(&side) || !side.is_multiple_of(2) {
        return None;
    }
    let solid = generate_triforce(side)?;
    let deleted = pierced_orbit(side)?;
    let is_deleted = |r: usize, c: usize| deleted.iter().any(|&(dr, dc)| (dr, dc) == (r, c));

    // Renumber survivors densely, preserving row-major order.
    let mut new_id = vec![None::<NodeId>; solid.nodes.len()];
    let mut nodes = Vec::new();
    for node in &solid.nodes {
        let [r, c] = node.axial.expect("triforce nodes carry axial");
        if is_deleted(r as usize, c as usize) {
            continue;
        }
        let id = nodes.len() as NodeId;
        new_id[node.id as usize] = Some(id);
        nodes.push(Node { id, ..node.clone() });
    }
    let mut edges: Vec<Edge> = solid
        .edges
        .iter()
        .filter_map(|e| {
            Some(Edge {
                a: new_id[e.a as usize]?,
                b: new_id[e.b as usize]?,
                kind: e.kind,
            })
        })
        .collect();

    // Re-triangulate each hexagonal hole: fan from the ring node farthest
    // from the board centroid (unique for every canonical orbit — checked
    // by the generator tests), which keeps mirror AND 120°-rotation maps
    // automorphisms because distance-to-centroid is symmetry-invariant.
    let centroid = {
        let n = solid.nodes.len() as f32;
        let (sx, sz) = solid.nodes.iter().fold((0.0, 0.0), |(x, z), nd| {
            (x + nd.position[0], z + nd.position[2])
        });
        (sx / n, sz / n)
    };
    let old_index = |r: usize, c: usize| -> NodeId { (r * (r + 1) / 2 + c) as NodeId };
    for &(r, c) in &deleted {
        // Ring in cyclic order around the hole.
        let ring_rc = [
            (r, c - 1),
            (r - 1, c - 1),
            (r - 1, c),
            (r, c + 1),
            (r + 1, c + 1),
            (r + 1, c),
        ];
        let ring: Vec<NodeId> = ring_rc
            .iter()
            .map(|&(rr, cc)| new_id[old_index(rr, cc) as usize].expect("ring survives"))
            .collect();
        let apex = (0..6)
            .max_by(|&i, &j| {
                let d = |k: usize| {
                    let p = &nodes[ring[k] as usize].position;
                    let (dx, dz) = (p[0] - centroid.0, p[2] - centroid.1);
                    dx * dx + dz * dz
                };
                d(i).partial_cmp(&d(j)).expect("finite positions")
            })
            .expect("six ring nodes");
        for k in [2usize, 3, 4] {
            edges.push(Edge {
                a: ring[apex],
                b: ring[(apex + k) % 6],
                kind: EdgeKind::IntraRealm,
            });
        }
    }
    Some(BoardDefinition {
        id: format!("tf{side}-v5p"),
        version: 1,
        nodes,
        edges,
        origins: Vec::new(),
    })
}
