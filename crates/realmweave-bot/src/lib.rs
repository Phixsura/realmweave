//! A playable opponent for weave-sever-v2 (and classic).
//!
//! 2-ply minimax over a connection-based evaluation with move filtering.
//! Not a strong engine — a *sparring partner*: it builds toward its weave,
//! blocks yours when cheap, cuts your bridges at the right moments, and
//! never plays illegal or pointless moves. Deterministic given (state,
//! seed) so games remain replayable.

pub mod mcts;

use realmweave_core::board::BoardGraph;
use realmweave_core::rules;
use realmweave_core::state::{GameResult, Move};
use realmweave_core::{Game, NodeId, Player};

/// Deterministic tie-break noise.
fn noise(seed: u64, ply: u32, k: usize) -> f64 {
    let mut x = seed
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(ply as u64)
        .wrapping_mul(0xBF58476D1CE4E5B9)
        .wrapping_add(k as u64 + 1);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 29;
    (x as f64) / (u64::MAX as f64)
}

/// Per-node entry cost for `player` route-finding. Contested ground —
/// empties adjacent to enemy stones — is more expensive: routes that hug
/// the opponent are fragile (one enemy stone or cut closes them), so a
/// careful player detours or reinforces. This single term is what bends
/// play from straight lines into Go-like shapes.
fn entry_cost(game: &Game, player: Player, node: NodeId) -> Option<u32> {
    let st = game.state();
    if st.is_petrified(node) {
        // Opponent fossils are free roads; your own are walls.
        return if st.fossil_road_for(node, player) {
            Some(0)
        } else {
            None
        };
    }
    match st.occupant(node) {
        Some(p) if p == player => Some(0),
        Some(_) => None,
        None => {
            let bd = game.board();
            let enemy_adj = bd
                .live_neighbors(node, &st.cut_edges)
                .filter(|&nb| st.occupant(nb) == Some(player.opponent()))
                .count() as u32;
            // Deterministic terrain grain (0..=1): breaks the tie between
            // the many equal-cost axis paths so routes meander naturally.
            let grain = {
                let mut x = (node as u64 + 1).wrapping_mul(0x9E3779B97F4A7C15);
                x ^= x >> 33;
                (x % 2) as u32
            };
            // base 4 per empty; +2 per adjacent enemy stone (cap +6)
            Some(4 + grain + 2 * enemy_adj.min(3))
        }
    }
}

/// Dijkstra from `from` to `to` over live edges with contested-ground
/// costs. Returns (cost, path-nodes) or None if unreachable.
fn route(game: &Game, player: Player, from: NodeId, to: NodeId) -> Option<(i64, Vec<NodeId>)> {
    let bd = game.board();
    let st = game.state();
    let n = bd.node_count();
    let mut dist = vec![i64::MAX; n];
    let mut prev = vec![NodeId::MAX; n];
    let mut heap = std::collections::BinaryHeap::new();
    dist[from as usize] = 0;
    heap.push(std::cmp::Reverse((0i64, from)));
    while let Some(std::cmp::Reverse((d, cur))) = heap.pop() {
        if cur == to {
            let mut path = vec![to];
            let mut c = to;
            while prev[c as usize] != NodeId::MAX {
                c = prev[c as usize];
                path.push(c);
            }
            path.reverse();
            return Some((d, path));
        }
        if d > dist[cur as usize] {
            continue;
        }
        for nb in bd.live_neighbors(cur, &st.cut_edges) {
            let Some(cost) = entry_cost(game, player, nb) else {
                continue;
            };
            let nd = d + cost as i64;
            if nd < dist[nb as usize] {
                dist[nb as usize] = nd;
                prev[nb as usize] = cur;
                heap.push(std::cmp::Reverse((nd, nb)));
            }
        }
    }
    None
}

/// Redundancy-aware link cost for one origin pair: cheapest route, plus
/// half the cost of the best *alternative* route that avoids the first
/// route's empty nodes. A single thin line scores much worse than a web —
/// this is what makes the bot build shapes instead of marching.
fn pair_cost(game: &Game, player: Player, from: NodeId, to: NodeId) -> i64 {
    const UNREACHABLE: i64 = 240;
    let Some((best, path)) = route(game, player, from, to) else {
        return UNREACHABLE;
    };
    // Second path: forbid the first path's empty interior nodes.
    let st = game.state();
    let blocked: Vec<NodeId> = path
        .iter()
        .copied()
        .filter(|&nd| st.occupant(nd).is_none() && nd != from && nd != to)
        .collect();
    let alt = route_avoiding(game, player, from, to, &blocked).map(|(c, _)| c);
    // If no alternative exists the connection hangs by a thread.
    best + alt.unwrap_or(UNREACHABLE / 2) / 2
}

fn route_avoiding(
    game: &Game,
    player: Player,
    from: NodeId,
    to: NodeId,
    blocked: &[NodeId],
) -> Option<(i64, Vec<NodeId>)> {
    let bd = game.board();
    let st = game.state();
    let n = bd.node_count();
    let mut dist = vec![i64::MAX; n];
    let mut prev = vec![NodeId::MAX; n];
    let mut heap = std::collections::BinaryHeap::new();
    dist[from as usize] = 0;
    heap.push(std::cmp::Reverse((0i64, from)));
    while let Some(std::cmp::Reverse((d, cur))) = heap.pop() {
        if cur == to {
            let mut path = vec![to];
            let mut c = to;
            while prev[c as usize] != NodeId::MAX {
                c = prev[c as usize];
                path.push(c);
            }
            path.reverse();
            return Some((d, path));
        }
        if d > dist[cur as usize] {
            continue;
        }
        for nb in bd.live_neighbors(cur, &st.cut_edges) {
            if blocked.contains(&nb) {
                continue;
            }
            let Some(cost) = entry_cost(game, player, nb) else {
                continue;
            };
            let nd = d + cost as i64;
            if nd < dist[nb as usize] {
                dist[nb as usize] = nd;
                prev[nb as usize] = cur;
                heap.push(std::cmp::Reverse((nd, nb)));
            }
        }
    }
    None
}

/// Redundancy-aware cost to link all origin pairs (lower = healthier).
/// Public so UIs can narrate how a move changed each side's position.
/// For trinity boards (no origins) this is the Y-connection cost summed
/// over realms instead.
pub fn link_cost(game: &Game, player: Player) -> i64 {
    let bd = game.board();
    let origins = bd.definition().origins_of(player);
    if origins.is_empty() {
        return trinity_cost(game, player);
    }
    let mut total = 0i64;
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            total += pair_cost(game, player, origins[i], origins[j]);
        }
    }
    total
}

/// Trinity Y: per realm, the cheapest tree linking all three sides
/// (approximated as min over side-A nodes of dist(A→B) + dist(A→C) style
/// multi-source BFS: cost from each side, summed at the best junction).
fn trinity_cost(game: &Game, player: Player) -> i64 {
    let bd = game.board();
    let st = game.state();
    let n = bd.node_count();
    // Triforce: one realm spanning the board; trinity: three partitions.
    let triforce = game.config().ruleset_id == rules::TRIFORCE_V5;
    let per_realm = if triforce { n } else { n / 3 };
    let side_len = (((8 * per_realm + 1) as f64).sqrt() as usize - 1) / 2;
    // Entry cost with the same anti-ruler medicine as the hex eval:
    // contested empties cost more, and deterministic terrain grain breaks
    // the tie between the many equal-length straight paths — without it
    // BFS always prefers axis-aligned routes and the bot fills rulers.
    let entry = |node: NodeId| -> Option<i64> {
        match st.occupant(node) {
            Some(p) if p == player => Some(0),
            Some(_) => None,
            None => {
                let enemy_adj = bd
                    .neighbors(node)
                    .iter()
                    .filter(|&&nb| st.occupant(nb) == Some(player.opponent()))
                    .count() as i64;
                let grain = {
                    let mut x = (node as u64 + 1).wrapping_mul(0x9E3779B97F4A7C15);
                    x ^= x >> 33;
                    (x % 2) as i64
                };
                // Edge-hugging penalty: on a triangle the bottom row touches
                // all three sides, so a naked edge line is the "cheapest" Y —
                // and the weakest (Y wisdom: edge play loses to cutting).
                // Charge rim cells extra so routes arc through the interior.
                let sides = if triforce {
                    realmweave_core::boardgen::triforce_sides(side_len, node)
                } else {
                    realmweave_core::boardgen::trinity_sides(side_len, node)
                };
                let edge_pen = if sides != 0 { 6 } else { 0 };
                Some(4 + grain + edge_pen + 2 * enemy_adj.min(3))
            }
        }
    };
    let mut total = 0i64;
    let realms = if triforce { 1 } else { 3 };
    for realm in 0..realms {
        let lo = (realm * per_realm) as NodeId;
        let hi = lo + per_realm as NodeId;
        // dist from each of the 3 sides via Dijkstra over entry costs
        let mut dists: Vec<Vec<i64>> = Vec::new();
        for side_bit in [1u8, 2, 4] {
            let mut dist = vec![i64::MAX; n];
            let mut heap = std::collections::BinaryHeap::new();
            for start in lo..hi {
                let start_sides = if triforce {
                    realmweave_core::boardgen::triforce_sides(side_len, start)
                } else {
                    realmweave_core::boardgen::trinity_sides(side_len, start)
                };
                if start_sides & side_bit == 0 {
                    continue;
                }
                let Some(c) = entry(start) else { continue };
                if c < dist[start as usize] {
                    dist[start as usize] = c;
                    heap.push(std::cmp::Reverse((c, start)));
                }
            }
            while let Some(std::cmp::Reverse((d, cur))) = heap.pop() {
                if d > dist[cur as usize] {
                    continue;
                }
                for &nb in bd.neighbors(cur) {
                    if nb < lo || nb >= hi {
                        continue;
                    }
                    let Some(c) = entry(nb) else { continue };
                    let nd = d + c;
                    if nd < dist[nb as usize] {
                        dist[nb as usize] = nd;
                        heap.push(std::cmp::Reverse((nd, nb)));
                    }
                }
            }
            dists.push(dist);
        }
        // best junction node
        let mut best = 200i64;
        for v in lo..hi {
            let (a, b, c) = (
                dists[0][v as usize],
                dists[1][v as usize],
                dists[2][v as usize],
            );
            if a == i64::MAX || b == i64::MAX || c == i64::MAX {
                continue;
            }
            // junction counted once if empty (approx: subtract twice the
            // typical empty cost so the shared node isn't triple-billed)
            let overlap = match st.occupant(v) {
                None => 9,
                _ => 0,
            };
            best = best.min(a + b + c - overlap);
        }
        // realm already won by me = 0; by opponent = full penalty handled
        // naturally (no junction reachable → 200)
        total += best;
    }
    total
}

/// The current cheapest route nodes for `player` (all origin pairs),
/// for UI visualization of intentions.
pub fn best_routes(game: &Game, player: Player) -> Vec<NodeId> {
    let bd = game.board();
    let origins = bd.definition().origins_of(player);
    let mut out = Vec::new();
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            if let Some((_, path)) = route(game, player, origins[i], origins[j]) {
                out.extend(path);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Static evaluation from `me`'s perspective: lower own link cost is good,
/// higher opponent cost is good; scissors are worth keeping.
/// Stones in atari (groups with exactly one liberty) for `player` —
/// death-rule pressure metric (trinity boards).
fn atari_stones(game: &Game, player: Player) -> u32 {
    let bd = game.board();
    let st = game.state();
    let n = bd.node_count();
    let mut visited = vec![false; n];
    let mut total = 0u32;
    for start in 0..n as NodeId {
        if st.occupant(start) != Some(player) || visited[start as usize] {
            continue;
        }
        let mut members = vec![start];
        visited[start as usize] = true;
        let mut queue = std::collections::VecDeque::from([start]);
        let mut libs = std::collections::HashSet::new();
        while let Some(cur) = queue.pop_front() {
            for &nb in bd.neighbors(cur) {
                match st.occupant(nb) {
                    None => {
                        libs.insert(nb);
                    }
                    Some(p) if p == player && !visited[nb as usize] => {
                        visited[nb as usize] = true;
                        queue.push_back(nb);
                        members.push(nb);
                    }
                    _ => {}
                }
            }
        }
        if libs.len() == 1 {
            total += members.len() as u32;
        }
    }
    total
}

fn evaluate(game: &Game, me: Player) -> f64 {
    if let Some(result) = game.result() {
        return match result {
            GameResult::Win { player, .. } if player == me => 10_000.0,
            GameResult::Win { .. } => -10_000.0,
            GameResult::Draw => 0.0,
        };
    }
    let mine = link_cost(game, me) as f64;
    let theirs = link_cost(game, me.opponent()) as f64;
    let scissor_value = game.state().scissors[match me {
        Player::Light => 0,
        Player::Dark => 1,
    }] as f64;
    let layer_lead = game.state().layers[match me {
        Player::Light => 0,
        Player::Dark => 1,
    }] as f64
        - game.state().layers[match me {
            Player::Light => 1,
            Player::Dark => 0,
        }] as f64;
    // Defense-weighted: protecting your own web matters more than hurting
    // theirs — aggression-first weights degenerate into strangle races.
    // Death-rule pressure (trinity): my stones in atari are near-lost,
    // enemy ataris are near-captured. Captured stones already improved
    // link costs; this term makes the THREAT visible one ply earlier.
    let atari_term = if game.board().definition().origins.is_empty() {
        2.0 * (atari_stones(game, me.opponent()) as f64 - atari_stones(game, me) as f64)
    } else {
        0.0
    };
    theirs - 2.4 * mine + 1.2 * scissor_value + 900.0 * layer_lead + atari_term
}

/// Candidate moves worth considering (keeps branching manageable):
/// placements near the action + cuts on the opponent's likely paths.
fn candidates(game: &Game, me: Player) -> Vec<Move> {
    let bd = game.board();
    let st = game.state();
    let mut out = Vec::new();

    // Plan-following: empty nodes on my current cheapest origin routes.
    // Without these the bot goes blind after petrification reshapes the
    // board (frontier stones are far from any useful path) and late game
    // degenerates into dead filler.
    for n in best_routes(game, me) {
        if st.occupant(n).is_none() && !st.is_petrified(n) {
            out.push(Move::Place(n));
        }
    }

    // Placements: empty nodes adjacent to any stone, or near origins.
    let mut near_action = vec![false; bd.node_count()];
    for n in 0..bd.node_count() as NodeId {
        if st.occupant(n).is_some() {
            near_action[n as usize] = true;
            for nb in bd.live_neighbors(n, &st.cut_edges) {
                near_action[nb as usize] = true;
            }
        }
    }
    for n in 0..bd.node_count() as NodeId {
        if st.occupant(n).is_none() && !st.is_petrified(n) && near_action[n as usize] {
            out.push(Move::Place(n));
        }
    }
    if out.is_empty() {
        // opening: play near own origins/gates (trinity: anywhere)
        let origins = bd.definition().origins_of(me);
        if origins.is_empty() {
            for n in 0..bd.node_count() as NodeId {
                if st.occupant(n).is_none() {
                    out.push(Move::Place(n));
                }
            }
        }
        for &o in &origins {
            for nb in bd.live_neighbors(o, &st.cut_edges) {
                if st.occupant(nb).is_none() && !st.is_petrified(nb) {
                    out.push(Move::Place(nb));
                }
            }
        }
    }

    out.sort_unstable_by_key(|m| match m {
        Move::Place(n) => *n as u32,
        _ => u32::MAX,
    });
    out.dedup();

    // Cuts: only when we have scissors — target edges between/next to
    // opponent stones (their bridges) or on their cheapest path.
    let my_scissors = st.scissors[match me {
        Player::Light => 0,
        Player::Dark => 1,
    }];
    if my_scissors > 0 {
        let opp = me.opponent();
        for (ei, edge) in bd.definition().edges.iter().enumerate() {
            let e = ei as u32;
            if st.cut_edges.contains(&e) {
                continue;
            }
            let a_opp = st.occupant(edge.a) == Some(opp);
            let b_opp = st.occupant(edge.b) == Some(opp);
            // any edge touching an enemy stone is a cut candidate
            if a_opp || b_opp {
                out.push(Move::CutEdge(e));
            }
        }
    }
    out
}

/// Shape score for placing at `node`: Go players avoid solid straight
/// chains (inefficient, cuttable in one sweep) and love diagonal contact
/// (尖) and one-point jumps — each stone adds new routes instead of
/// lengthening one. Returns a penalty (negative) for straight-chain
/// extension and a small bonus for diagonal relationships.
fn shape_score(game: &Game, me: Player, node: NodeId) -> f64 {
    let bd = game.board();
    let def = bd.definition();
    let Some(ax) = def.nodes[node as usize].axial else {
        return 0.0;
    };
    let realm = def.nodes[node as usize].realm;
    let st = game.state();
    // Straight directions in this board's coordinate system. Hex realms
    // use axial coords; trinity realms use (row, col) on a triangular
    // grid, whose six neighbor steps are below — using hex dirs there
    // means the anti-march terms simply never fire (the "always straight
    // lines" bug).
    let is_trinity = def.origins.is_empty();
    const HEX_DIRS: [[i32; 2]; 6] = [[1, 0], [0, 1], [-1, 1], [-1, 0], [0, -1], [1, -1]];
    const TRI_DIRS: [[i32; 2]; 6] = [[0, 1], [0, -1], [1, 0], [-1, 0], [1, 1], [-1, -1]];
    let dirs: [[i32; 2]; 6] = if is_trinity { TRI_DIRS } else { HEX_DIRS };
    let occ_at = |a: [i32; 2]| -> bool {
        bd.axial_index()
            .get(&(realm, a))
            .map(|&id| st.occupant(id) == Some(me))
            .unwrap_or(false)
    };
    let mut score = 0.0;
    let mut solid_contacts = 0;
    for d in dirs {
        let b1 = [ax[0] - d[0], ax[1] - d[1]];
        let b2 = [ax[0] - 2 * d[0], ax[1] - 2 * d[1]];
        if occ_at(b1) {
            solid_contacts += 1;
            if occ_at(b2) {
                score -= 14.0; // third stone in a straight line: don't march
            }
        }
        // One-point jump (拆一): own stone two away with the gap free.
        let gap_free = bd
            .axial_index()
            .get(&(realm, b1))
            .map(|&id| st.occupant(id).is_none() && !st.is_petrified(id))
            .unwrap_or(false);
        if gap_free && occ_at(b2) {
            score += 1.5;
        }
    }
    // Heavy clumping is also not 步步为营: more than 2 solid contacts is slow.
    if solid_contacts >= 3 {
        score -= 2.0;
    }
    // Diagonal (尖): "diagonals" are dir_i + dir_{i+1} of adjacent dirs.
    for i in 0..6 {
        let d1 = dirs[i];
        let d2 = dirs[(i + 1) % 6];
        let diag = [ax[0] + d1[0] + d2[0], ax[1] + d1[1] + d2[1]];
        let via_a = [ax[0] + d1[0], ax[1] + d1[1]];
        let via_b = [ax[0] + d2[0], ax[1] + d2[1]];
        let free = |a: [i32; 2]| {
            bd.axial_index()
                .get(&(realm, a))
                .map(|&id| st.occupant(id).is_none() && !st.is_petrified(id))
                .unwrap_or(false)
        };
        if occ_at(diag) && free(via_a) && free(via_b) {
            score += 1.2; // 尖: two ways to connect, springy shape
        }
    }
    score
}

/// Choose the bot's move: 2-ply search (my move → opponent's best reply by
/// static eval) over filtered candidates.
pub fn choose_move(game: &Game, seed: u64) -> Option<Move> {
    choose_move_with_budget(game, seed, mcts::MctsConfig::default())
}

/// Like [`choose_move`] with an explicit MCTS budget (trinity only; other
/// rulesets use the fixed 2-ply search).
pub fn choose_move_with_budget(game: &Game, seed: u64, budget: mcts::MctsConfig) -> Option<Move> {
    // Y-family boards get the real engine: UCT over a fast simulator.
    if game.board().definition().origins.is_empty()
        && matches!(
            game.config().ruleset_id.as_str(),
            rules::TRINITY_Y_V4 | rules::TRIFORCE_V5
        )
    {
        // The simulator plays simple-ko; the engine enforces positional
        // superko. A rules divergence here must never leak to callers: an
        // unplayable "best move" would livelock deterministic retry loops.
        if let Some(mv) = mcts::choose_move_mcts(game, seed, budget) {
            if game.validate(&mv).is_ok() {
                return Some(mv);
            }
            // Rare: superko-illegal choice. Take the best legal placement
            // by a quick re-search with a different seed, else any legal.
            if let Some(mv2) = mcts::choose_move_mcts(game, seed.wrapping_add(0x9E3779B9), budget) {
                if mv2 != mv && game.validate(&mv2).is_ok() {
                    return Some(mv2);
                }
            }
            return game
                .legal_moves()
                .into_iter()
                .find(|m| matches!(m, Move::Place(_)))
                .or(Some(Move::Pass));
        }
        // MCTS found no candidate (every empty is an own eye or superko-
        // masked). Never pass while a legal placement exists: mutual
        // passing would end an undecided game as a draw the Y theorem says
        // someone can still win.
        return game
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m, Move::Place(_)))
            .or(Some(Move::Pass));
    }
    let me = game.to_move();
    let cands: Vec<Move> = candidates(game, me)
        .into_iter()
        .filter(|m| game.validate(m).is_ok())
        .collect();
    if cands.is_empty() {
        return game
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m, Move::Place(_) | Move::Pass));
    }
    let ply = game.state().ply;

    // Pass 1: static score, keep top 12.
    let mut scored: Vec<(f64, Move)> = Vec::new();
    for (k, &mv) in cands.iter().enumerate() {
        let bd = BoardGraph::new(game.board().definition().clone()).ok()?;
        let mut sim = Game::replay(bd, game.config().clone(), &game.state().move_log).ok()?;
        if sim.play(mv).is_err() {
            continue;
        }
        let shape = match mv {
            Move::Place(n) => shape_score(game, me, n),
            _ => 0.0,
        };
        let s = evaluate(&sim, me) + shape + noise(seed, ply, k) * 0.3;
        scored.push((s, mv));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(12);

    // Pass 2: opponent's best static reply (from THEIR candidate set).
    // The shape term must survive into the final pick, or pass 1's
    // anti-march filtering is undone right here.
    let mut best: Option<(f64, Move)> = None;
    for &(_, mv) in &scored {
        let shape = match mv {
            Move::Place(n) => shape_score(game, me, n),
            _ => 0.0,
        };
        let bd = BoardGraph::new(game.board().definition().clone()).ok()?;
        let mut sim = Game::replay(bd, game.config().clone(), &game.state().move_log).ok()?;
        if sim.play(mv).is_err() {
            continue;
        }
        let after = if sim.result().is_some() {
            evaluate(&sim, me)
        } else {
            // opponent minimizes my eval
            let opp = me.opponent();
            let mut worst = f64::INFINITY;
            let reply_cands: Vec<Move> = candidates(&sim, opp)
                .into_iter()
                .filter(|m| sim.validate(m).is_ok())
                .take(16)
                .collect();
            if reply_cands.is_empty() {
                evaluate(&sim, me)
            } else {
                for rmv in reply_cands {
                    let bd2 = BoardGraph::new(sim.board().definition().clone()).ok()?;
                    let mut sim2 =
                        Game::replay(bd2, sim.config().clone(), &sim.state().move_log).ok()?;
                    if sim2.play(rmv).is_err() {
                        continue;
                    }
                    let v = evaluate(&sim2, me);
                    if v < worst {
                        worst = v;
                    }
                }
                if worst.is_finite() {
                    worst
                } else {
                    evaluate(&sim, me)
                }
            }
        };
        let after = after + shape;
        if best.map(|(b, _)| after > b).unwrap_or(true) {
            best = Some((after, mv));
        }
    }
    best.map(|(_, m)| m)
        .or_else(|| scored.first().map(|(_, m)| *m))
}

/// Convenience: does this game's ruleset support the bot?
pub fn supports(ruleset_id: &str) -> bool {
    matches!(
        ruleset_id,
        rules::WEAVE_SEVER_V2
            | rules::WEAVE_LAYERS_V3
            | rules::TRINITY_Y_V4
            | rules::TRIFORCE_V5
            | rules::THREE_REALMS_V1
            | rules::SEVER_V1
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use realmweave_core::{boardgen, GameConfig};

    /// The chosen move must always be playable on the real engine — the
    /// simulator's simple-ko vs the engine's superko must never leak.
    #[test]
    fn chosen_moves_always_playable() {
        let def = boardgen::generate_trinity(6).unwrap();
        let board = BoardGraph::new(def).unwrap();
        let cfg = GameConfig::new(board.definition().id.clone()).with_ruleset(rules::TRINITY_Y_V4);
        let mut game = Game::new(board, cfg).unwrap();
        let budget = mcts::MctsConfig {
            playouts: 60,
            c: 0.9,
        };
        for ply in 0..80 {
            if game.result().is_some() {
                break;
            }
            let mv = choose_move_with_budget(&game, 0x5EED ^ ply, budget)
                .expect("bot always proposes something");
            assert!(
                game.play(mv).is_ok(),
                "ply {ply}: bot proposed unplayable {mv:?}"
            );
        }
    }
}

#[cfg(test)]
mod superko_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use realmweave_core::rules::position_hash;
    use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move};

    /// The engine's recorded position hashes and the MCTS root filter must
    /// agree byte-for-byte, including the to_move timing subtlety (the
    /// engine hashes BEFORE flipping to_move). If this drifts, the superko
    /// filter silently stops filtering.
    #[test]
    fn engine_hash_timing_contract() {
        let def = boardgen::generate_trinity(6).unwrap();
        let board = BoardGraph::new(def).unwrap();
        let cfg = GameConfig::new(board.definition().id.clone())
            .with_ruleset(realmweave_core::rules::TRINITY_Y_V4);
        let mut game = Game::new(board, cfg).unwrap();
        let mv = game
            .legal_moves()
            .into_iter()
            .find(|m| matches!(m, Move::Place(_)))
            .unwrap();
        let mover = game.to_move();
        game.play(mv).unwrap();
        let engine_hash = *game.state().position_hashes.last().unwrap();
        // Recompute the same hash the way the MCTS filter does: current
        // occupancy + the MOVER (not the new to_move).
        let mut shell = realmweave_core::GameState::new(
            game.board().definition().id.clone(),
            game.board().node_count(),
        );
        shell.occupancy = game.state().occupancy.clone();
        shell.to_move = mover;
        assert_eq!(position_hash(&shell), engine_hash);
        // And the flipped to_move must NOT match — proving the timing matters.
        shell.to_move = mover.opponent();
        assert_ne!(position_hash(&shell), engine_hash);
    }
}
