//! Monte-Carlo tree search for Trinity Y.
//!
//! A dedicated fast simulator (raw occupancy vectors, incremental liberty
//! checks, no superko bookkeeping inside playouts) drives a UCT tree. The
//! playout policy is uniform-random over non-eye empties — deliberately
//! dumb and fast; strength comes from the tree, not the policy.

use realmweave_core::rules::position_hash;
use realmweave_core::{boardgen, BoardGraph, Game, GameState, Move, NodeId, Player};

/// Search budget: number of playouts from the root.
#[derive(Clone, Copy, Debug)]
pub struct MctsConfig {
    /// Playouts per move decision.
    pub playouts: u32,
    /// UCT exploration constant.
    pub c: f64,
}

impl Default for MctsConfig {
    fn default() -> Self {
        MctsConfig {
            playouts: 3000,
            c: 0.9,
        }
    }
}

/// Board family the simulator is playing.
#[derive(Clone, Copy, PartialEq)]
enum SimMode {
    /// Three separate triangles; two realm-Ys win (trinity-y-v4).
    Trinity,
    /// One merged triangle; a single big Y wins (triforce-v5).
    Triforce,
}

/// Fast Y-family position: enough state for legal playouts, nothing more.
#[derive(Clone)]
struct Sim<'a> {
    board: &'a BoardGraph,
    mode: SimMode,
    side: usize,
    per_realm: usize,
    occ: Vec<Option<Player>>,
    /// Realm winner, if decided ([realm] -> Option<Player>).
    realm_won: [Option<Player>; 3],
    to_move: Player,
    winner: Option<Player>,
    /// Simple ko: the single point just captured (illegal immediate refill).
    /// Used inside playouts, where full superko would be too slow and a
    /// stochastic estimate tolerates the noise.
    ko_point: Option<NodeId>,
}

/// The engine's exact position hash for a Sim state — occupancy + to_move
/// through `rules::position_hash`, so hashes are comparable with the live
/// game's `position_hashes` history. Tree-level moves are checked against
/// that set: what the tree recommends, the engine will accept.
///
/// TIMING: the engine pushes its hash BEFORE flipping `to_move` (the hash
/// carries the MOVER), while `Sim::place` flips at the end — so callers
/// pass the mover explicitly rather than trusting `sim.to_move`.
fn sim_hash(sim: &Sim, board_id: &str, mover: Player) -> u64 {
    // Reuse the engine's function via a minimal GameState shell so the two
    // implementations can never drift apart.
    let mut st = GameState::new(board_id.to_string(), sim.occ.len());
    st.occupancy = sim.occ.clone();
    st.to_move = mover;
    position_hash(&st)
}

impl<'a> Sim<'a> {
    fn from_game(game: &'a Game) -> Self {
        let board = game.board();
        let mode = if game.config().ruleset_id == realmweave_core::rules::TRIFORCE_V5 {
            SimMode::Triforce
        } else {
            SimMode::Trinity
        };
        let n = board.node_count();
        let per_realm = match mode {
            SimMode::Trinity => n / 3,
            SimMode::Triforce => n, // one realm spanning the whole board
        };
        let side = match mode {
            SimMode::Trinity => (((8 * per_realm + 1) as f64).sqrt() as usize - 1) / 2,
            SimMode::Triforce => boardgen::tf_side_len(board.definition()),
        };
        let st = game.state();
        let mut sim = Sim {
            board,
            mode,
            side,
            per_realm,
            occ: st.occupancy.clone(),
            realm_won: [None; 3],
            to_move: st.to_move,
            winner: None,
            ko_point: None,
        };
        let realms = match mode {
            SimMode::Trinity => 3,
            SimMode::Triforce => 1,
        };
        for realm in 0..realms {
            sim.realm_won[realm] = sim.realm_winner(realm);
        }
        sim.update_match_winner();
        sim
    }

    fn realm_of(&self, node: NodeId) -> usize {
        node as usize / self.per_realm
    }

    /// Side bitmask under the current mode's geometry.
    fn sides_of(&self, node: NodeId) -> u8 {
        match self.mode {
            SimMode::Trinity => boardgen::trinity_sides(self.side, node),
            SimMode::Triforce => boardgen::triforce_sides(self.board.definition(), self.side, node),
        }
    }

    /// One group's members + liberty flag (early-exits on second liberty).
    fn group_alive(&self, start: NodeId, buf: &mut Vec<NodeId>, seen: &mut [bool]) -> bool {
        let Some(player) = self.occ[start as usize] else {
            return true;
        };
        buf.clear();
        buf.push(start);
        seen[start as usize] = true;
        let mut i = 0;
        let mut alive = false;
        while i < buf.len() {
            let cur = buf[i];
            i += 1;
            for &nb in self.board.neighbors(cur) {
                match self.occ[nb as usize] {
                    None => alive = true,
                    Some(p) if p == player && !seen[nb as usize] => {
                        seen[nb as usize] = true;
                        buf.push(nb);
                    }
                    _ => {}
                }
            }
        }
        alive
    }

    /// Apply a placement; returns false if it was suicide/ko (not applied).
    fn place(&mut self, node: NodeId, scratch: &mut Scratch) -> bool {
        if self.occ[node as usize].is_some() || self.realm_won[self.realm_of(node)].is_some() {
            return false;
        }
        if self.ko_point == Some(node) {
            return false;
        }
        let me = self.to_move;
        self.occ[node as usize] = Some(me);
        // capture adjacent enemy groups with no liberties
        let mut captured_total = 0usize;
        let mut last_captured = None;
        scratch.seen.iter_mut().for_each(|b| *b = false);
        for k in 0..self.board.neighbors(node).len() {
            let nb = self.board.neighbors(node)[k];
            if self.occ[nb as usize] == Some(me.opponent())
                && !scratch.seen[nb as usize]
                && !self.group_alive(nb, &mut scratch.buf, &mut scratch.seen)
            {
                for &m in &scratch.buf {
                    self.occ[m as usize] = None;
                    captured_total += 1;
                    last_captured = Some(m);
                }
            }
        }
        // suicide?
        scratch.seen.iter_mut().for_each(|b| *b = false);
        if !self.group_alive(node, &mut scratch.buf, &mut scratch.seen) {
            self.occ[node as usize] = None;
            // restore captures? none happened if we're suicidal (capturing
            // would have given us a liberty), so nothing to undo.
            return false;
        }
        self.ko_point = if captured_total == 1 {
            self.ko_after_single_capture(node, me, last_captured)
        } else {
            None
        };
        // realm win check (only the placed realm can newly complete)
        let realm = self.realm_of(node);
        if self.realm_won[realm].is_none() {
            self.realm_won[realm] = self.realm_winner_from(realm, node, me);
            if self.realm_won[realm].is_some() {
                self.update_match_winner();
            }
        }
        self.to_move = me.opponent();
        true
    }

    /// Simple ko: a single capture by a LONE stone that itself sits in
    /// atari (exactly one liberty — the captured point). Without the
    /// liberty check this also bans legal snapback recaptures (lone
    /// capturer with 2+ liberties), systematically misevaluating capture
    /// races in every playout.
    fn ko_after_single_capture(
        &self,
        node: NodeId,
        me: Player,
        last_captured: Option<NodeId>,
    ) -> Option<NodeId> {
        let mut libs = 0;
        for &nb in self.board.neighbors(node) {
            match self.occ[nb as usize] {
                None => libs += 1,
                Some(p) if p == me => return None, // not lone
                _ => {}
            }
        }
        (libs == 1).then_some(last_captured).flatten()
    }

    fn update_match_winner(&mut self) {
        let need = match self.mode {
            SimMode::Trinity => 2,
            SimMode::Triforce => 1,
        };
        for pl in [Player::Light, Player::Dark] {
            if self.realm_won.iter().filter(|w| **w == Some(pl)).count() >= need {
                self.winner = Some(pl);
            }
        }
    }

    /// Did `player`'s group through `node` just complete a Y in `realm`?
    fn realm_winner_from(&self, realm: usize, node: NodeId, player: Player) -> Option<Player> {
        let lo = (realm * self.per_realm) as NodeId;
        let hi = lo + self.per_realm as NodeId;
        let mut stack = vec![node];
        let mut seen = vec![false; self.per_realm];
        seen[(node - lo) as usize] = true;
        let mut touch = self.sides_of(node);
        while let Some(cur) = stack.pop() {
            for &nb in self.board.neighbors(cur) {
                if nb < lo || nb >= hi {
                    continue;
                }
                if !seen[(nb - lo) as usize] && self.occ[nb as usize] == Some(player) {
                    seen[(nb - lo) as usize] = true;
                    touch |= self.sides_of(nb);
                    stack.push(nb);
                }
            }
        }
        (touch == 7).then_some(player)
    }

    /// Full scan (used only at root construction).
    fn realm_winner(&self, realm: usize) -> Option<Player> {
        let lo = (realm * self.per_realm) as NodeId;
        let hi = lo + self.per_realm as NodeId;
        for pl in [Player::Light, Player::Dark] {
            let mut seen = vec![false; self.per_realm];
            for start in lo..hi {
                if self.occ[start as usize] != Some(pl) || seen[(start - lo) as usize] {
                    continue;
                }
                if self.realm_winner_from(realm, start, pl) == Some(pl) {
                    return Some(pl);
                }
                // mark component visited
                let mut stack = vec![start];
                seen[(start - lo) as usize] = true;
                while let Some(cur) = stack.pop() {
                    for &nb in self.board.neighbors(cur) {
                        if nb >= lo
                            && nb < hi
                            && !seen[(nb - lo) as usize]
                            && self.occ[nb as usize] == Some(pl)
                        {
                            seen[(nb - lo) as usize] = true;
                            stack.push(nb);
                        }
                    }
                }
            }
        }
        None
    }

    /// True eye for `player`: empty point whose neighbors are all own
    /// stones. Filling it is never right in a playout.
    fn is_own_eye(&self, node: NodeId, player: Player) -> bool {
        self.board
            .neighbors(node)
            .iter()
            .all(|&nb| self.occ[nb as usize] == Some(player))
    }

    /// Random playout to the end; returns the winner (draw broken by
    /// realm count then coin-parity for termination guarantees).
    fn playout(&mut self, rng: &mut u64, scratch: &mut Scratch, max_moves: u32) -> Player {
        let n = self.board.node_count();
        let mut passes = 0u32;
        for _ in 0..max_moves {
            if let Some(w) = self.winner {
                return w;
            }
            // pick a random legal, non-eye empty
            let mut tries = 0;
            let mut placed = false;
            while tries < 12 {
                *rng ^= *rng << 13;
                *rng ^= *rng >> 7;
                *rng ^= *rng << 17;
                let node = (*rng % n as u64) as NodeId;
                if self.occ[node as usize].is_none()
                    && self.realm_won[self.realm_of(node)].is_none()
                    && !self.is_own_eye(node, self.to_move)
                    && self.place(node, scratch)
                {
                    placed = true;
                    passes = 0;
                    break;
                }
                tries += 1;
            }
            if !placed {
                // dense board: linear scan fallback
                let start = (*rng % n as u64) as usize;
                let mut found = false;
                for off in 0..n {
                    let node = ((start + off) % n) as NodeId;
                    if self.occ[node as usize].is_none()
                        && self.realm_won[self.realm_of(node)].is_none()
                        && !self.is_own_eye(node, self.to_move)
                        && self.place(node, scratch)
                    {
                        found = true;
                        passes = 0;
                        break;
                    }
                }
                if !found {
                    self.to_move = self.to_move.opponent();
                    // Ko forbids only the IMMEDIATE recapture; an
                    // intervening pass re-opens the point.
                    self.ko_point = None;
                    passes += 1;
                    if passes >= 2 {
                        break;
                    }
                }
            }
        }
        if let Some(w) = self.winner {
            return w;
        }
        // score by realms won
        let l = self
            .realm_won
            .iter()
            .filter(|w| **w == Some(Player::Light))
            .count();
        let d = self
            .realm_won
            .iter()
            .filter(|w| **w == Some(Player::Dark))
            .count();
        match l.cmp(&d) {
            std::cmp::Ordering::Greater => Player::Light,
            std::cmp::Ordering::Less => Player::Dark,
            // Undecided at the cap: a COIN FLIP, not a tempo rule. The
            // move cap is even, so "to_move.opponent()" resolved every
            // truncated playout from a given node to the SAME side —
            // systematic credit for a position nobody evaluated. Noise is
            // honest; parity is a thumb on the scale.
            std::cmp::Ordering::Equal => {
                *rng ^= *rng << 13;
                *rng ^= *rng >> 7;
                *rng ^= *rng << 17;
                if *rng & 1 == 0 {
                    Player::Light
                } else {
                    Player::Dark
                }
            }
        }
    }

    fn legal_candidates(&self) -> Vec<NodeId> {
        (0..self.board.node_count() as NodeId)
            .filter(|&n| {
                self.occ[n as usize].is_none()
                    && self.realm_won[self.realm_of(n)].is_none()
                    && !self.is_own_eye(n, self.to_move)
            })
            .collect()
    }
}

struct Scratch {
    buf: Vec<NodeId>,
    seen: Vec<bool>,
}

struct NodeStats {
    visits: u32,
    wins: f64,
    /// Move that led here (root children).
    mv: NodeId,
}

/// Choose a trinity move by UCT search. Returns None on positions with no
/// legal placement (caller falls back to Pass).
pub fn choose_move_mcts(game: &Game, seed: u64, config: MctsConfig) -> Option<Move> {
    choose_move_mcts_scored(game, seed, config).map(|(mv, _)| mv)
}

/// Like [`choose_move_mcts`], also returning the chosen move's estimated
/// win rate for the side to move (its root visit-average). Drives the
/// pie-rule swap decision: compare the best placement's win rate against
/// the position's value after swapping.
pub fn choose_move_mcts_scored(game: &Game, seed: u64, config: MctsConfig) -> Option<(Move, f64)> {
    let root = Sim::from_game(game);
    let me = root.to_move;
    let board_id = game.board().definition().id.clone();
    // Positional superko, exactly as the engine sees it: the game's full
    // hash history. Any root move recreating one of these is excluded from
    // the tree, so the recommendation is engine-legal by construction.
    let history: std::collections::HashSet<u64> =
        game.state().position_hashes.iter().copied().collect();
    let mut scratch0 = Scratch {
        buf: Vec::with_capacity(64),
        seen: vec![false; game.board().node_count()],
    };
    let cands: Vec<NodeId> = root
        .legal_candidates()
        .into_iter()
        .filter(|&mv| {
            let mut sim = root.clone();
            sim.place(mv, &mut scratch0) && !history.contains(&sim_hash(&sim, &board_id, me))
        })
        .collect();
    if cands.is_empty() {
        return None;
    }
    if cands.len() == 1 {
        // No search ran; 0.5 = "no information", not a confident estimate.
        return Some((Move::Place(cands[0]), 0.5));
    }
    let n = game.board().node_count();
    let mut scratch = Scratch {
        buf: Vec::with_capacity(64),
        seen: vec![false; n],
    };
    let mut stats: Vec<NodeStats> = cands
        .iter()
        .map(|&mv| NodeStats {
            visits: 0,
            wins: 0.0,
            mv,
        })
        .collect();
    let mut rng = seed | 1;
    let mut total = 0u32;
    let max_playout_moves = (n as u32) * 2;
    for _ in 0..config.playouts {
        // select child by UCT
        let mut best = 0usize;
        let mut best_score = f64::MIN;
        for (i, s) in stats.iter().enumerate() {
            let score = if s.visits == 0 {
                // Unvisited first, in index order. (Note: f64 cannot
                // represent MAX - i distinctly; the strict `>` comparison
                // is what keeps the FIRST unvisited child selected.)
                f64::MAX
            } else {
                s.wins / s.visits as f64
                    + config.c * ((total.max(1) as f64).ln() / s.visits as f64).sqrt()
            };
            if score > best_score {
                best_score = score;
                best = i;
            }
        }
        // simulate
        let mut sim = root.clone();
        if !sim.place(stats[best].mv, &mut scratch) {
            // illegal under ko subtleties: mark as lost cause
            stats[best].visits += 1;
            total += 1;
            continue;
        }
        let winner = sim.playout(&mut rng, &mut scratch, max_playout_moves);
        stats[best].visits += 1;
        if winner == me {
            stats[best].wins += 1.0;
        }
        total += 1;
    }
    stats.iter().max_by_key(|s| s.visits).map(|s| {
        let rate = if s.visits > 0 {
            s.wins / s.visits as f64
        } else {
            0.5
        };
        (Move::Place(s.mv), rate)
    })
}
