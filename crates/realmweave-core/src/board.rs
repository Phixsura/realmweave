//! Board definition and graph types.
//!
//! The board is pure data: nodes, edges, origins. Game legality never depends
//! on rendering coordinates; `position` exists only as a default layout hint.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub type NodeId = u16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Realm {
    Heaven,
    Mortal,
    Underworld,
}

impl Realm {
    pub const ALL: [Realm; 3] = [Realm::Heaven, Realm::Mortal, Realm::Underworld];

    pub fn index(self) -> usize {
        match self {
            Realm::Heaven => 0,
            Realm::Mortal => 1,
            Realm::Underworld => 2,
        }
    }

    pub fn from_index(i: usize) -> Option<Realm> {
        Realm::ALL.get(i).copied()
    }

    /// Realms directly reachable through portals (adjacent layers only).
    pub fn is_adjacent(self, other: Realm) -> bool {
        let d = self.index().abs_diff(other.index());
        d == 1
    }

    pub fn name(self) -> &'static str {
        match self {
            Realm::Heaven => "Heaven",
            Realm::Mortal => "Mortal",
            Realm::Underworld => "Underworld",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Player {
    Light,
    Dark,
}

impl Player {
    pub fn opponent(self) -> Player {
        match self {
            Player::Light => Player::Dark,
            Player::Dark => Player::Light,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Player::Light => "Light",
            Player::Dark => "Dark",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub realm: Realm,
    /// Default layout hint (x, layer-y, z). Never used for legality.
    pub position: [f32; 3],
    /// Axial hex coordinate within the realm, when the board is hex-generated.
    /// Used for exact symmetry validation; optional for hand-made boards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axial: Option<[i32; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    IntraRealm,
    Portal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edge {
    pub a: NodeId,
    pub b: NodeId,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub player: Player,
    pub node: NodeId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoardDefinition {
    /// Stable identifier, e.g. "hex37-v1".
    pub id: String,
    /// Schema/content version of this board definition.
    pub version: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub origins: Vec<Origin>,
}

impl BoardDefinition {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn origins_of(&self, player: Player) -> Vec<NodeId> {
        self.origins
            .iter()
            .filter(|o| o.player == player)
            .map(|o| o.node)
            .collect()
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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
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

    pub fn definition(&self) -> &BoardDefinition {
        &self.definition
    }

    pub fn node_count(&self) -> usize {
        self.adjacency.len()
    }

    pub fn neighbors(&self, node: NodeId) -> &[NodeId] {
        &self.adjacency[node as usize]
    }

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
            let d = dist[cur as usize].unwrap();
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
