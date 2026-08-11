//! Board definition and graph types.
//!
//! The board is pure data: nodes, edges, origins. Game legality never depends
//! on rendering coordinates; `position` exists only as a default layout hint.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Dense node identifier: always `0..node_count` within a board.
pub type NodeId = u16;

/// One of the three stacked realms of a Realmweave board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Realm {
    /// Top layer.
    Heaven,
    /// Middle layer.
    Mortal,
    /// Bottom layer.
    Underworld,
}

impl Realm {
    /// All realms in layer order (top to bottom).
    pub const ALL: [Realm; 3] = [Realm::Heaven, Realm::Mortal, Realm::Underworld];

    /// Layer index (0 = Heaven, 1 = Mortal, 2 = Underworld).
    pub fn index(self) -> usize {
        match self {
            Realm::Heaven => 0,
            Realm::Mortal => 1,
            Realm::Underworld => 2,
        }
    }

    /// Inverse of [`Realm::index`].
    pub fn from_index(i: usize) -> Option<Realm> {
        Realm::ALL.get(i).copied()
    }

    /// Realms directly reachable through portals (adjacent layers only).
    pub fn is_adjacent(self, other: Realm) -> bool {
        let d = self.index().abs_diff(other.index());
        d == 1
    }

    /// English display name.
    pub fn name(self) -> &'static str {
        match self {
            Realm::Heaven => "Heaven",
            Realm::Mortal => "Mortal",
            Realm::Underworld => "Underworld",
        }
    }
}

/// The two players. Light always moves first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Player {
    /// First player.
    Light,
    /// Second player.
    Dark,
}

impl Player {
    /// The other player.
    pub fn opponent(self) -> Player {
        match self {
            Player::Light => Player::Dark,
            Player::Dark => Player::Light,
        }
    }

    /// English display name.
    pub fn name(self) -> &'static str {
        match self {
            Player::Light => "Light",
            Player::Dark => "Dark",
        }
    }
}

/// One vertex of the board graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    /// Dense id (`0..node_count`).
    pub id: NodeId,
    /// Which realm this node belongs to.
    pub realm: Realm,
    /// Default layout hint (x, layer-y, z). Never used for legality.
    pub position: [f32; 3],
    /// Axial hex coordinate within the realm, when the board is hex-generated.
    /// Used for exact symmetry validation; optional for hand-made boards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axial: Option<[i32; 2]>,
}

/// Whether an edge stays within a realm or crosses between realms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    /// Connects two nodes of the same realm.
    IntraRealm,
    /// Gate column segment linking adjacent realms.
    Portal,
}

/// Undirected edge of the board graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edge {
    /// One endpoint.
    pub a: NodeId,
    /// The other endpoint.
    pub b: NodeId,
    /// Intra-realm or portal.
    pub kind: EdgeKind,
}

impl Edge {
    /// Canonical unordered key for duplicate detection.
    pub fn key(&self) -> (NodeId, NodeId) {
        if self.a <= self.b {
            (self.a, self.b)
        } else {
            (self.b, self.a)
        }
    }
}

/// A player's origin node (pre-occupied, immovable; one per realm).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// Owner of this origin.
    pub player: Player,
    /// The origin's node.
    pub node: NodeId,
}

/// Structural family of a board — determines goal geometry, symmetry
/// checks, and rendering layout. Derived from the id in ONE place so the
/// `starts_with("tf")` string checks scattered through three crates have a
/// single authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardFamily {
    /// Three stacked hexagonal realms joined at gate columns (origins).
    StackedHex,
    /// Three separate triangles, side goals (trinity-y-v4).
    SplitTriangles,
    /// One merged triangle: realms + weave-heart as interior regions
    /// (triforce-v5).
    MergedTriangle,
}

/// A complete board as data: the unit of persistence and validation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoardDefinition {
    /// Stable identifier, e.g. "hex37-v1".
    pub id: String,
    /// Schema/content version of this board definition.
    pub version: u32,
    /// All vertices, ids dense `0..len`.
    pub nodes: Vec<Node>,
    /// All undirected edges.
    pub edges: Vec<Edge>,
    /// Both players' origins (may be empty for side-goal boards).
    pub origins: Vec<Origin>,
}

impl BoardDefinition {
    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The given player's origin nodes.
    pub fn origins_of(&self, player: Player) -> Vec<NodeId> {
        self.origins
            .iter()
            .filter(|o| o.player == player)
            .map(|o| o.node)
            .collect()
    }

    /// Structural family, derived from the id convention
    /// (`hex…`/`tri…`/`tf…`). Unknown prefixes default to StackedHex, the
    /// family of every hand-made board to date.
    pub fn family(&self) -> BoardFamily {
        if self.id.starts_with("tf") {
            BoardFamily::MergedTriangle
        } else if self.id.starts_with("tri") {
            BoardFamily::SplitTriangles
        } else {
            BoardFamily::StackedHex
        }
    }

    /// Content fingerprint: hash of nodes/edges/origins (not id/version).
    /// Records carry this so a future generator change that silently
    /// alters a board's content is DETECTED at replay time instead of
    /// producing a subtly different game. Uses the stable FNV hasher —
    /// this value is PERSISTED in records, and DefaultHasher's algorithm
    /// may change across Rust releases (which would flag every archived
    /// record as BoardDrift after a toolchain upgrade).
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = crate::rules::StableHasher::default();
        for n in &self.nodes {
            n.id.hash(&mut h);
            n.realm.hash(&mut h);
            n.axial.map(|a| (a[0], a[1])).hash(&mut h);
        }
        for e in &self.edges {
            e.key().hash(&mut h);
            e.kind.hash(&mut h);
        }
        for o in &self.origins {
            o.player.hash(&mut h);
            o.node.hash(&mut h);
        }
        h.finish()
    }

    /// All nodes that are endpoints of portal edges ("gates").
    pub fn gate_nodes(&self) -> Vec<NodeId> {
        let mut gates: Vec<NodeId> = self
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Portal)
            .flat_map(|e| [e.a, e.b])
            .collect();
        gates.sort_unstable();
        gates.dedup();
        gates
    }
}

/// Immutable adjacency structure built once from a `BoardDefinition`.
///
/// Node ids are required to be dense: `0..nodes.len()`.
#[derive(Clone, Debug)]
pub struct BoardGraph {
    definition: BoardDefinition,
    adjacency: Vec<Vec<NodeId>>,
    realm_of: Vec<Realm>,
}

/// Structural errors detected when building a [`BoardGraph`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[allow(missing_docs)] // each variant's #[error] text IS its documentation
pub enum BoardError {
    #[error("node ids must be dense 0..{expected}, found id {found}")]
    NonDenseIds { expected: usize, found: NodeId },
    #[error("duplicate node id {0}")]
    DuplicateNodeId(NodeId),
    #[error("edge references unknown node {0}")]
    UnknownNode(NodeId),
    #[error("board has no nodes")]
    Empty,
}

impl BoardGraph {
    /// Build the adjacency structure, validating id density and edge
    /// endpoints.
    pub fn new(definition: BoardDefinition) -> Result<Self, BoardError> {
        let n = definition.nodes.len();
        if n == 0 {
            return Err(BoardError::Empty);
        }
        let mut seen = vec![false; n];
        for node in &definition.nodes {
            let idx = node.id as usize;
            if idx >= n {
                return Err(BoardError::NonDenseIds {
                    expected: n,
                    found: node.id,
                });
            }
            if seen[idx] {
                return Err(BoardError::DuplicateNodeId(node.id));
            }
            seen[idx] = true;
        }
        let mut realm_of = vec![Realm::Mortal; n];
        for node in &definition.nodes {
            realm_of[node.id as usize] = node.realm;
        }
        let mut adjacency = vec![Vec::new(); n];
        for edge in &definition.edges {
            for id in [edge.a, edge.b] {
                if id as usize >= n {
                    return Err(BoardError::UnknownNode(id));
                }
            }
            adjacency[edge.a as usize].push(edge.b);
            adjacency[edge.b as usize].push(edge.a);
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
            neighbors.dedup();
        }
        Ok(BoardGraph {
            definition,
            adjacency,
            realm_of,
        })
    }

    /// The underlying data definition.
    pub fn definition(&self) -> &BoardDefinition {
        &self.definition
    }

    /// Number of nodes.
    pub fn node_count(&self) -> usize {
        self.adjacency.len()
    }

    /// Sorted, deduplicated neighbor list of `node`.
    pub fn neighbors(&self, node: NodeId) -> &[NodeId] {
        &self.adjacency[node as usize]
    }

    /// Realm of `node`.
    pub fn realm_of(&self, node: NodeId) -> Realm {
        self.realm_of[node as usize]
    }

    /// Breadth-first distances from `start` over the full (empty-board) graph.
    pub fn bfs_distances(&self, start: NodeId) -> Vec<Option<u32>> {
        let mut dist = vec![None; self.node_count()];
        let mut queue = std::collections::VecDeque::new();
        dist[start as usize] = Some(0);
        queue.push_back(start);
        while let Some(cur) = queue.pop_front() {
            let Some(d) = dist[cur as usize] else {
                continue; // unreachable: queued nodes always have a distance
            };
            for &next in self.neighbors(cur) {
                if dist[next as usize].is_none() {
                    dist[next as usize] = Some(d + 1);
                    queue.push_back(next);
                }
            }
        }
        dist
    }

    /// Adjacency filtered by a set of removed edges (weave-sever-v2).
    /// O(cuts) per neighbor query is fine: cuts ≤ 6 per game.
    pub fn live_neighbors<'a>(
        &'a self,
        node: NodeId,
        cut_edges: &'a [u32],
    ) -> impl Iterator<Item = NodeId> + 'a {
        let def = self.definition();
        self.neighbors(node).iter().copied().filter(move |&nb| {
            !cut_edges.iter().any(|&ci| {
                let e = &def.edges[ci as usize];
                (e.a == node && e.b == nb) || (e.a == nb && e.b == node)
            })
        })
    }

    /// Lookup from axial coordinate + realm to node id, when axial data exists.
    pub fn axial_index(&self) -> HashMap<(Realm, [i32; 2]), NodeId> {
        self.definition
            .nodes
            .iter()
            .filter_map(|n| n.axial.map(|ax| ((n.realm, ax), n.id)))
            .collect()
    }
}
