//! Graph-level fairness analysis: origin/gate distances, degree profiles,
//! minimum vertex cuts, and vertex-disjoint route counts.

use std::collections::HashMap;

use realmweave_core::{BoardGraph, NodeId, Player};

pub fn report(board: &BoardGraph) -> String {
    let def = board.definition();
    let gates = def.gate_nodes();
    let mut out = String::new();
    out.push_str(&format!("fairness report for {}\n", def.id));

    for player in [Player::Light, Player::Dark] {
        let origins = def.origins_of(player);
        out.push_str(&format!("\n{}:\n", player.name()));
        // Pairwise origin distances.
        let mut pairwise = Vec::new();
        for i in 0..origins.len() {
            let dist = board.bfs_distances(origins[i]);
            for j in (i + 1)..origins.len() {
                pairwise.push(dist[origins[j] as usize].unwrap_or(u32::MAX));
            }
        }
        pairwise.sort_unstable();
        out.push_str(&format!("  origin pairwise distances: {pairwise:?}\n"));
        // Distance to nearest gate per origin.
        let mut to_gates = Vec::new();
        for &o in &origins {
            let dist = board.bfs_distances(o);
            let nearest = gates
                .iter()
                .filter_map(|&g| dist[g as usize])
                .min()
                .unwrap_or(u32::MAX);
            to_gates.push(nearest);
        }
        to_gates.sort_unstable();
        out.push_str(&format!(
            "  nearest-gate distance per origin: {to_gates:?}\n"
        ));
        // Min vertex cut + disjoint routes between each origin pair.
        let mut cuts = Vec::new();
        for i in 0..origins.len() {
            for j in (i + 1)..origins.len() {
                cuts.push(min_vertex_cut(board, origins[i], origins[j]));
            }
        }
        cuts.sort_unstable();
        out.push_str(&format!(
            "  min vertex cuts between origin pairs (= max disjoint routes): {cuts:?}\n"
        ));
    }

    // Degree histogram.
    let mut hist: HashMap<usize, usize> = HashMap::new();
    for node in &def.nodes {
        *hist.entry(board.neighbors(node.id).len()).or_insert(0) += 1;
    }
    let mut degrees: Vec<_> = hist.into_iter().collect();
    degrees.sort_unstable();
    out.push_str("\ndegree histogram (degree: count):\n");
    for (d, c) in degrees {
        out.push_str(&format!("  {d}: {c}\n"));
    }

    // Super-node detection: nodes whose removal disconnects an origin pair.
    let mut critical = Vec::new();
    for player in [Player::Light, Player::Dark] {
        let origins = def.origins_of(player);
        for i in 0..origins.len() {
            for j in (i + 1)..origins.len() {
                if min_vertex_cut(board, origins[i], origins[j]) == 1 {
                    critical.push((player, origins[i], origins[j]));
                }
            }
        }
    }
    if critical.is_empty() {
        out.push_str("\nno single-node bottlenecks between any origin pair\n");
    } else {
        out.push_str(&format!(
            "\nWARNING single-node bottlenecks: {critical:?}\n"
        ));
    }
    out
}

/// Minimum vertex cut between s and t (excluding endpoints) via unit-capacity
/// max-flow on the node-split graph. By Menger's theorem this equals the
/// number of internally vertex-disjoint routes.
pub fn min_vertex_cut(board: &BoardGraph, s: NodeId, t: NodeId) -> u32 {
    // Node splitting: each node v becomes v_in (2v) and v_out (2v+1) with a
    // capacity-1 arc v_in→v_out (except s, t which get infinite capacity).
    // Each undirected edge (u,v) becomes u_out→v_in and v_out→u_in, cap 1
    // (capacities on edges can be 1 since vertex caps dominate).
    let n = board.node_count();
    let num = 2 * n;
    // adjacency with (to, cap, rev_index)
    let mut graph: Vec<Vec<(usize, u32, usize)>> = vec![Vec::new(); num];
    let add_edge = |graph: &mut Vec<Vec<(usize, u32, usize)>>, a: usize, b: usize, cap: u32| {
        let ra = graph[b].len();
        let rb = graph[a].len();
        graph[a].push((b, cap, ra));
        graph[b].push((a, 0, rb));
    };
    let inf = u32::MAX / 2;
    for v in 0..n {
        let cap = if v == s as usize || v == t as usize {
            inf
        } else {
            1
        };
        add_edge(&mut graph, 2 * v, 2 * v + 1, cap);
    }
    for node in 0..n {
        for &nb in board.neighbors(node as NodeId) {
            // Directed both ways; add once per ordered pair.
            add_edge(&mut graph, 2 * node + 1, 2 * (nb as usize), inf);
        }
    }
    let source = 2 * (s as usize) + 1;
    let sink = 2 * (t as usize);

    // Dinic-lite (BFS levels + DFS blocking flow); graph is tiny.
    let mut flow = 0u32;
    loop {
        // BFS levels.
        let mut level = vec![usize::MAX; num];
        let mut queue = std::collections::VecDeque::new();
        level[source] = 0;
        queue.push_back(source);
        while let Some(u) = queue.pop_front() {
            for &(v, cap, _) in &graph[u] {
                if cap > 0 && level[v] == usize::MAX {
                    level[v] = level[u] + 1;
                    queue.push_back(v);
                }
            }
        }
        if level[sink] == usize::MAX {
            break;
        }
        // DFS blocking flow.
        fn dfs(
            graph: &mut Vec<Vec<(usize, u32, usize)>>,
            level: &[usize],
            iter: &mut [usize],
            u: usize,
            sink: usize,
            pushed: u32,
        ) -> u32 {
            if u == sink {
                return pushed;
            }
            while iter[u] < graph[u].len() {
                let (v, cap, rev) = graph[u][iter[u]];
                if cap > 0 && level[v] == level[u] + 1 {
                    let d = dfs(graph, level, iter, v, sink, pushed.min(cap));
                    if d > 0 {
                        graph[u][iter[u]].1 -= d;
                        graph[v][rev].1 += d;
                        return d;
                    }
                }
                iter[u] += 1;
            }
            0
        }
        let mut iter = vec![0usize; num];
        loop {
            let pushed = dfs(&mut graph, &level, &mut iter, source, sink, u32::MAX);
            if pushed == 0 {
                break;
            }
            flow += pushed;
        }
    }
    flow
}
