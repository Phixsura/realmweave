//! Shared graph predicates used by multiple rule sets: connectivity,
//! weave detection, strangle predicates, position hashing.

use std::collections::VecDeque;

use crate::board::{BoardGraph, NodeId, Player};
use crate::state::GameState;

/// Whether all of `player`'s origins can still be joined through own
/// stones + empty nodes over the live graph (enemy stones block).
pub fn potential_connected(board: &BoardGraph, state: &GameState, player: Player) -> bool {
    potential_origin_groups(board, state, player) == 1
}

/// v3 permanence-based doom: origins separated by PERMANENT terrain only —
/// cut edges and petrified nodes. Enemy stones are not permanent walls in
/// the layers game (networks petrify away), so they slow you down but
/// cannot doom you. Strangle must be carved into the world itself.
pub fn permanently_connected(board: &BoardGraph, state: &GameState, player: Player) -> bool {
    let origins = board.definition().origins_of(player);
    let Some(&first) = origins.first() else {
        return true;
    };
    let mut visited = vec![false; board.node_count()];
    let mut queue = VecDeque::new();
    visited[first as usize] = true;
    queue.push_back(first);
    while let Some(cur) = queue.pop_front() {
        for next in board.live_neighbors(cur, &state.cut_edges) {
            let blocked = state.is_petrified(next) && !state.fossil_road_for(next, player);
            if !visited[next as usize] && !blocked {
                visited[next as usize] = true;
                queue.push_back(next);
            }
        }
    }
    origins.iter().all(|&o| visited[o as usize])
}

/// Number of groups the player's origins fall into under potential
/// connectivity (1 = all connectable, 3 = fully strangled).
pub fn potential_origin_groups(board: &BoardGraph, state: &GameState, player: Player) -> u32 {
    let origins = board.definition().origins_of(player);
    let passable = |n: NodeId| {
        if state.is_petrified(n) {
            return state.fossil_road_for(n, player);
        }
        match state.occupant(n) {
            Some(p) => p == player,
            None => true,
        }
    };
    let mut groups = 0u32;
    let mut assigned = vec![false; origins.len()];
    for i in 0..origins.len() {
        if assigned[i] {
            continue;
        }
        groups += 1;
        assigned[i] = true;
        // BFS from origins[i]; mark other origins reached.
        let mut visited = vec![false; board.node_count()];
        let mut queue = VecDeque::new();
        visited[origins[i] as usize] = true;
        queue.push_back(origins[i]);
        while let Some(cur) = queue.pop_front() {
            for next in board.live_neighbors(cur, &state.cut_edges) {
                if !visited[next as usize] && passable(next) {
                    visited[next as usize] = true;
                    queue.push_back(next);
                }
            }
        }
        for (j, assigned_j) in assigned.iter_mut().enumerate().skip(i + 1) {
            if visited[origins[j] as usize] {
                *assigned_j = true;
            }
        }
    }
    groups
}

/// Would cutting edge `e` strangle `player`'s own origins?
pub(crate) fn cut_self_strangles(
    board: &BoardGraph,
    state: &GameState,
    player: Player,
    e: u32,
    permanent_only: bool,
) -> bool {
    let mut sim = state.clone();
    sim.position_hashes = Vec::new(); // not needed for the check
    sim.cut_edges.push(e);
    if permanent_only {
        !permanently_connected(board, &sim, player)
    } else {
        !potential_connected(board, &sim, player)
    }
}

/// Realm weave over the LIVE graph (cut edges removed). Opponent fossils
/// count as traversable links in your weave — the enemy's dead network is
/// your infrastructure (v3's anti-snowball rule).
pub fn live_realm_weave(board: &BoardGraph, state: &GameState, player: Player) -> bool {
    let origins = board.definition().origins_of(player);
    let Some(&first) = origins.first() else {
        return false;
    };
    if state.occupant(first) != Some(player) {
        return false;
    }
    let mut visited = vec![false; board.node_count()];
    let mut queue = VecDeque::new();
    visited[first as usize] = true;
    queue.push_back(first);
    while let Some(cur) = queue.pop_front() {
        for next in board.live_neighbors(cur, &state.cut_edges) {
            let mine = state.occupant(next) == Some(player);
            let road = state.fossil_road_for(next, player);
            if !visited[next as usize] && (mine || road) {
                visited[next as usize] = true;
                queue.push_back(next);
            }
        }
    }
    origins.iter().all(|&o| visited[o as usize])
}

// ------------------------------------------------------------- helpers ---

/// Order-independent hash of the current position: occupancy + to_move.
/// Used for positional-superko (ko) checks.
pub fn position_hash(state: &GameState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    state.occupancy.hash(&mut h);
    state.to_move.hash(&mut h);
    h.finish()
}

/// `player`'s connected component (sorted) containing `start`.
pub fn connected_component(
    board: &BoardGraph,
    state: &GameState,
    player: Player,
    start: NodeId,
) -> Vec<NodeId> {
    if state.occupant(start) != Some(player) {
        return Vec::new();
    }
    let mut visited = vec![false; board.node_count()];
    let mut component = Vec::new();
    let mut queue = VecDeque::new();
    visited[start as usize] = true;
    queue.push_back(start);
    while let Some(cur) = queue.pop_front() {
        component.push(cur);
        for &next in board.neighbors(cur) {
            if !visited[next as usize] && state.occupant(next) == Some(player) {
                visited[next as usize] = true;
                queue.push_back(next);
            }
        }
    }
    component.sort_unstable();
    component
}

/// All connected components of a player's network.
pub fn player_components(
    board: &BoardGraph,
    state: &GameState,
    player: Player,
) -> Vec<Vec<NodeId>> {
    let mut seen = vec![false; board.node_count()];
    let mut components = Vec::new();
    for node in 0..board.node_count() as NodeId {
        if state.occupant(node) == Some(player) && !seen[node as usize] {
            let component = connected_component(board, state, player, node);
            for &n in &component {
                seen[n as usize] = true;
            }
            components.push(component);
        }
    }
    components
}

/// True when all three of the player's origins share one connected component
/// of the player's network (single-route weave).
pub fn has_realm_weave(board: &BoardGraph, state: &GameState, player: Player) -> bool {
    let origins = board.definition().origins_of(player);
    let Some(&first) = origins.first() else {
        return false;
    };
    let component = connected_component(board, state, player, first);
    origins.iter().all(|o| component.binary_search(o).is_ok())
}

/// True when every origin pair is connected by at least `required`
/// internally-vertex-disjoint routes through the player's own network
/// (Menger: pairwise vertex connectivity ≥ `required`).
pub fn has_weave_routes(
    board: &BoardGraph,
    state: &GameState,
    player: Player,
    required: u32,
) -> bool {
    if !has_realm_weave(board, state, player) {
        return false;
    }
    let origins = board.definition().origins_of(player);
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            if vertex_disjoint_routes(board, state, player, origins[i], origins[j], required)
                < required
            {
                return false;
            }
        }
    }
    true
}

/// Max-flow (capped at `cap`) on the node-split subgraph induced by the
/// player's stones: the number of internally-vertex-disjoint s–t routes.
fn vertex_disjoint_routes(
    board: &BoardGraph,
    state: &GameState,
    player: Player,
    s: NodeId,
    t: NodeId,
    cap: u32,
) -> u32 {
    let n = board.node_count();
    let num = 2 * n;
    let mut graph: Vec<Vec<(usize, u32, usize)>> = vec![Vec::new(); num];
    let add_edge = |graph: &mut Vec<Vec<(usize, u32, usize)>>, a: usize, b: usize, c: u32| {
        let ra = graph[b].len();
        let rb = graph[a].len();
        graph[a].push((b, c, ra));
        graph[b].push((a, 0, rb));
    };
    for v in 0..n {
        if state.occupant(v as NodeId) != Some(player) {
            continue;
        }
        let c = if v == s as usize || v == t as usize {
            cap
        } else {
            1
        };
        add_edge(&mut graph, 2 * v, 2 * v + 1, c);
    }
    for v in 0..n {
        if state.occupant(v as NodeId) != Some(player) {
            continue;
        }
        for &nb in board.neighbors(v as NodeId) {
            if state.occupant(nb) == Some(player) {
                add_edge(&mut graph, 2 * v + 1, 2 * (nb as usize), cap);
            }
        }
    }
    let source = 2 * (s as usize) + 1;
    let sink = 2 * (t as usize);

    let mut flow = 0u32;
    while flow < cap {
        // BFS augmenting path (unit capacities → Edmonds-Karp is fine).
        let mut prev: Vec<Option<(usize, usize)>> = vec![None; num];
        let mut queue = VecDeque::new();
        queue.push_back(source);
        let mut reached = false;
        while let Some(u) = queue.pop_front() {
            if u == sink {
                reached = true;
                break;
            }
            for (ei, &(v, c, _)) in graph[u].iter().enumerate() {
                if c > 0 && prev[v].is_none() && v != source {
                    prev[v] = Some((u, ei));
                    queue.push_back(v);
                }
            }
        }
        if !reached {
            break;
        }
        // Augment by 1.
        let mut v = sink;
        while v != source {
            let Some((u, ei)) = prev[v] else {
                break; // reconstructed path always has predecessors
            };
            let rev = graph[u][ei].2;
            graph[u][ei].1 -= 1;
            graph[v][rev].1 += 1;
            v = u;
        }
        flow += 1;
    }
    flow
}
