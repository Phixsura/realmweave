//! Ruleset abstraction and the Three Realms rule family.
//!
//! The win rule is deliberately kept behind `RuleSet` so variants never
//! require touching generic state mutation, networking, or rendering.
//! All current variants are parameterizations of one `WeaveRules` engine:
//!
//! | id | routes | sever | territory | notes |
//! |---|---|---|---|---|
//! | `three-realms-v1` | 1 | – | – | original one-turn-confirmed weave |
//! | `three-realms-doubleweave-v1` | 2 | – | – | weave needs 2 vertex-disjoint routes |
//! | `three-realms-sever-v1` | 1 | 3 charges | – | stones can be removed |
//! | `three-realms-territory-v1` | – | – | yes | pass/score endgame |

use std::collections::VecDeque;

use crate::board::{BoardGraph, NodeId, Player};
use crate::state::{GameResult, GameState, Move, WinReason};

pub const THREE_REALMS_V1: &str = "three-realms-v1";
pub const DOUBLE_WEAVE_V1: &str = "three-realms-doubleweave-v1";
pub const SEVER_V1: &str = "three-realms-sever-v1";
pub const TERRITORY_V1: &str = "three-realms-territory-v1";
pub const SUPPLY_V1: &str = "three-realms-supply-v1";
pub const SUPPLY_RANGE_V1: &str = "three-realms-supplyrange-v1";
pub const WEAVE_SEVER_V2: &str = "weave-sever-v2";
pub const WEAVE_LAYERS_V3: &str = "weave-layers-v3";
pub const TRINITY_Y_V4: &str = "trinity-y-v4";

/// Weave bonus added to the largest-network score in the territory variant.
pub const TERRITORY_WEAVE_BONUS: i32 = 15;
/// Sever charges per player in the sever variant.
pub const SEVER_CHARGES: u8 = 3;
/// Komi (in half-points) granted to Dark in the supply variant. Sweep at
/// greedy level (hex91, 40 games/point): 0.5 komi → 50/50; larger komi
/// overshoots to Dark. The half-point's real job is eliminating draws —
/// supply first-move advantage is inherently small. Re-tune with stronger
/// bots and human play (Go needed a century to settle on 6.5/7.5).
pub const SUPPLY_KOMI_HALF: i32 = 1;
/// Weave bonus in the supply variant's area scoring.
pub const SUPPLY_WEAVE_BONUS: i32 = 10;
/// Scissors per player in weave-sever-v2. Origin-adjacent edges are
/// uncuttable, making the min origin-pair edge cut 8 (measured on all
/// standard boards) — pure-scissor strangling is impossible at K=3;
/// strangles require stone walls plus surgical cuts.
pub const SCISSORS: u8 = 3;
/// Scissors granted to BOTH players when a layer petrifies (v3).
pub const LAYER_SCISSORS: u8 = 2;
/// Scissors cap (v3).
pub const SCISSORS_CAP: u8 = 4;
/// Layers needed to win weave-layers-v3.
pub const LAYERS_TO_WIN: u8 = 3;
/// Hard ply cap for weave-layers-v3: at this many moves the game closes
/// and fallback scoring (layers first) decides. Keeps marathons bounded.
pub const V3_PLY_CAP: u32 = 500;
/// Supply range: max number of EMPTY nodes a supply line may cross
/// (supply-range variant). Stones must advance in linked steps — a stone
/// flung far from its network starves. This is what forces Go-like
/// incremental, mutually-supporting play.
pub const SUPPLY_RANGE: u32 = 4;
/// Hard ply cap for supply games (× node count): the game is scored as-is
/// when reached. Generous safety net against endless capture cycles that
/// superko cannot rule out.
pub const SUPPLY_PLY_CAP_FACTOR: u32 = 6;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuleError {
    #[error("game is already finished")]
    GameFinished,
    #[error("node {0} is occupied")]
    Occupied(NodeId),
    #[error("node {0} does not exist")]
    NoSuchNode(NodeId),
    #[error("swap is not available")]
    SwapUnavailable,
    #[error("sever is not available")]
    SeverUnavailable,
    #[error("node {0} cannot be severed")]
    CannotSever(NodeId),
    #[error("pass is not allowed in this ruleset")]
    PassUnavailable,
    #[error("move at {0} would leave your own group without supply")]
    SuicideMove(NodeId),
    #[error("move at {0} repeats a previous position (ko)")]
    KoViolation(NodeId),
    #[error("edge {0} cannot be cut")]
    CannotCut(u32),
    #[error("node {0} is adjacent to an enemy origin (sanctum)")]
    OriginSanctum(NodeId),
    #[error("no scissors remaining")]
    NoScissors,
    #[error("this cut would strangle your own origins")]
    SelfStrangle,
    #[error("unknown ruleset id {0}")]
    UnknownRuleset(String),
}

pub trait RuleSet: Send + Sync {
    /// Stable versioned identifier persisted with every game.
    fn id(&self) -> &str;
    /// One-time state initialization (e.g. sever charges).
    fn setup(&self, _state: &mut GameState) {}
    fn legal_moves(&self, board: &BoardGraph, state: &GameState) -> Vec<Move>;
    fn validate_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<(), RuleError>;
    fn apply_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<GameState, RuleError>;
    fn evaluate(&self, board: &BoardGraph, state: &GameState) -> Option<GameResult>;
}

/// Look up a ruleset implementation by its persisted id.
pub fn ruleset_by_id(id: &str, pie_rule: bool) -> Result<Box<dyn RuleSet>, RuleError> {
    match id {
        THREE_REALMS_V1 => Ok(Box::new(WeaveRules::classic(pie_rule))),
        DOUBLE_WEAVE_V1 => Ok(Box::new(WeaveRules::double_weave(pie_rule))),
        SEVER_V1 => Ok(Box::new(WeaveRules::sever(pie_rule))),
        TERRITORY_V1 => Ok(Box::new(WeaveRules::territory(pie_rule))),
        SUPPLY_V1 => Ok(Box::new(WeaveRules::supply(pie_rule))),
        SUPPLY_RANGE_V1 => Ok(Box::new(WeaveRules::supply_range(pie_rule))),
        WEAVE_SEVER_V2 => Ok(Box::new(WeaveSeverV2 {
            pie_rule,
            layers_to_win: 1,
        })),
        WEAVE_LAYERS_V3 => Ok(Box::new(WeaveSeverV2 {
            pie_rule,
            layers_to_win: LAYERS_TO_WIN,
        })),
        TRINITY_Y_V4 => Ok(Box::new(TrinityY { pie_rule })),
        other => Err(RuleError::UnknownRuleset(other.to_string())),
    }
}

/// All known ruleset ids (for tooling/UI).
pub const ALL_RULESETS: [&str; 7] = [
    WEAVE_SEVER_V2,
    THREE_REALMS_V1,
    DOUBLE_WEAVE_V1,
    SEVER_V1,
    TERRITORY_V1,
    SUPPLY_V1,
    SUPPLY_RANGE_V1,
];

/// Parameterized Three Realms rules. Shared mechanics:
/// - Players alternate turns; stones go on empty nodes; enemy stones block.
/// - Origins are pre-occupied and immovable.
/// - Optional pie rule (swap as Dark's first response; log-only, seats are
///   the caller's concern).
///
/// Weave variants (`territory == false`):
/// - A weave = all three own origins connected by `required_routes`
///   internally-vertex-disjoint routes through the player's own network.
/// - Completing it is provisional; it must survive one opponent turn.
/// - With sever charges, the opponent's response turn has real teeth.
/// - Full board with no confirmed weave: standing provisional weave wins,
///   otherwise draw.
///
/// Territory variant:
/// - No weave win. `Pass` is legal; two consecutive passes (or a full
///   board) end the game.
/// - Score = size of the player's largest connected network, plus
///   `TERRITORY_WEAVE_BONUS` if all three origins share one component.
///   Higher score wins; equal is a draw.
pub struct WeaveRules {
    id: &'static str,
    pub pie_rule: bool,
    required_routes: u32,
    sever_charges: u8,
    territory: bool,
    /// Supply variant: capture-by-encirclement + area scoring (see docs).
    supply: bool,
    /// Max empty nodes a supply line may cross (None = unlimited).
    supply_range: Option<u32>,
}

impl WeaveRules {
    pub fn classic(pie_rule: bool) -> Self {
        WeaveRules {
            id: THREE_REALMS_V1,
            pie_rule,
            required_routes: 1,
            sever_charges: 0,
            territory: false,
            supply: false,
            supply_range: None,
        }
    }

    pub fn double_weave(pie_rule: bool) -> Self {
        WeaveRules {
            id: DOUBLE_WEAVE_V1,
            pie_rule,
            required_routes: 2,
            sever_charges: 0,
            territory: false,
            supply: false,
            supply_range: None,
        }
    }

    pub fn sever(pie_rule: bool) -> Self {
        WeaveRules {
            id: SEVER_V1,
            pie_rule,
            required_routes: 1,
            sever_charges: SEVER_CHARGES,
            territory: false,
            supply: false,
            supply_range: None,
        }
    }

    pub fn territory(pie_rule: bool) -> Self {
        WeaveRules {
            id: TERRITORY_V1,
            pie_rule,
            required_routes: 1,
            sever_charges: 0,
            territory: true,
            supply: false,
            supply_range: None,
        }
    }

    /// Supply rules: like Go translated into connection language.
    /// - Every group must keep a *supply line*: a path through own stones
    ///   and empty nodes to one of its player's origins. After each
    ///   placement, enemy groups with no supply are captured (removed);
    ///   leaving your own group unsupplied is an illegal (suicide) move.
    /// - Positional superko: recreating any previous position is illegal.
    /// - Game ends on two consecutive passes (or the move cap); score =
    ///   stones + empty regions bordered only by you + weave bonus, with
    ///   komi to Dark. Higher score wins.
    pub fn supply(pie_rule: bool) -> Self {
        WeaveRules {
            id: SUPPLY_V1,
            pie_rule,
            required_routes: 1,
            sever_charges: 0,
            territory: false,
            supply: true,
            supply_range: None,
        }
    }

    /// Supply with a range limit: a supply line may cross at most
    /// `SUPPLY_RANGE` empty nodes. Forces step-by-step, mutually-supporting
    /// advances (the "Go feel"): distant lone stones starve.
    pub fn supply_range(pie_rule: bool) -> Self {
        WeaveRules {
            id: SUPPLY_RANGE_V1,
            pie_rule,
            required_routes: 1,
            sever_charges: 0,
            territory: false,
            supply: true,
            supply_range: Some(SUPPLY_RANGE),
        }
    }

    fn swap_available(&self, state: &GameState) -> bool {
        self.pie_rule && !state.swap_used && state.ply == 1 && state.to_move == Player::Dark
    }

    fn board_full(state: &GameState) -> bool {
        state.occupancy.iter().all(|occ| occ.is_some())
    }

    fn has_weave(&self, board: &BoardGraph, state: &GameState, player: Player) -> bool {
        if self.required_routes <= 1 {
            has_realm_weave(board, state, player)
        } else {
            has_weave_routes(board, state, player, self.required_routes)
        }
    }

    fn charges(state: &GameState, player: Player) -> u8 {
        state.sever_charges[player_index(player)]
    }

    /// Simulate a supply placement: reject suicide and superko repeats.
    fn supply_placement_ok(
        &self,
        board: &BoardGraph,
        state: &GameState,
        node: NodeId,
    ) -> Result<(), RuleError> {
        let mover = state.to_move;
        // Lightweight simulation: skip cloning the growing superko history
        // (it is only *read* from `state`), keeping legal-move generation
        // O(nodes) instead of O(nodes × plies).
        let mut sim = state.clone();
        sim.position_hashes = Vec::new();
        sim.occupancy[node as usize] = Some(mover);
        let _ = capture_unsupplied(board, &mut sim, mover.opponent(), self.supply_range);
        // Suicide: after enemy captures resolve, the mover's own group at
        // `node` must have supply.
        if !group_has_supply(board, &sim, mover, node, self.supply_range) {
            return Err(RuleError::SuicideMove(node));
        }
        // Positional superko.
        let hash = position_hash(&sim);
        if state.position_hashes.contains(&hash) {
            return Err(RuleError::KoViolation(node));
        }
        Ok(())
    }

    /// Area scoring: stones + exclusive empty regions + weave bonus; komi
    /// (in half-points) goes to Dark.
    fn score_supply(&self, board: &BoardGraph, state: &GameState) -> GameResult {
        let light = supply_score(board, state, Player::Light).total_half();
        let dark = supply_score(board, state, Player::Dark).total_half();
        match light.cmp(&dark) {
            std::cmp::Ordering::Greater => GameResult::Win {
                player: Player::Light,
                reason: WinReason::Territory,
            },
            std::cmp::Ordering::Less => GameResult::Win {
                player: Player::Dark,
                reason: WinReason::Territory,
            },
            std::cmp::Ordering::Equal => GameResult::Draw,
        }
    }

    fn score_territory(&self, board: &BoardGraph, state: &GameState) -> GameResult {
        let light = territory_score(board, state, Player::Light);
        let dark = territory_score(board, state, Player::Dark);
        match light.cmp(&dark) {
            std::cmp::Ordering::Greater => GameResult::Win {
                player: Player::Light,
                reason: WinReason::Territory,
            },
            std::cmp::Ordering::Less => GameResult::Win {
                player: Player::Dark,
                reason: WinReason::Territory,
            },
            std::cmp::Ordering::Equal => GameResult::Draw,
        }
    }
}

fn player_index(player: Player) -> usize {
    match player {
        Player::Light => 0,
        Player::Dark => 1,
    }
}

impl RuleSet for WeaveRules {
    fn id(&self) -> &str {
        self.id
    }

    fn setup(&self, state: &mut GameState) {
        state.sever_charges = [self.sever_charges, self.sever_charges];
    }

    fn legal_moves(&self, board: &BoardGraph, state: &GameState) -> Vec<Move> {
        if state.is_finished() {
            return Vec::new();
        }
        let mut moves: Vec<Move> = (0..board.node_count() as NodeId)
            .filter(|&n| state.occupant(n).is_none())
            .map(Move::Place)
            .collect();
        if Self::charges(state, state.to_move) > 0 {
            let enemy = state.to_move.opponent();
            let origins: Vec<NodeId> = board.definition().origins_of(enemy);
            moves.extend(
                (0..board.node_count() as NodeId)
                    .filter(|&n| state.occupant(n) == Some(enemy) && !origins.contains(&n))
                    .map(Move::Sever),
            );
        }
        if self.supply {
            // Filter placements that are suicide or repeat a position.
            moves.retain(|m| match m {
                Move::Place(n) => self.supply_placement_ok(board, state, *n).is_ok(),
                _ => true,
            });
        }
        if self.territory || self.supply {
            moves.push(Move::Pass);
        }
        if self.swap_available(state) {
            moves.push(Move::Swap);
        }
        moves.push(Move::Resign);
        moves
    }

    fn validate_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<(), RuleError> {
        if state.is_finished() {
            return Err(RuleError::GameFinished);
        }
        match mv {
            Move::Place(node) => {
                if *node as usize >= board.node_count() {
                    return Err(RuleError::NoSuchNode(*node));
                }
                if state.occupant(*node).is_some() {
                    return Err(RuleError::Occupied(*node));
                }
                if self.supply {
                    self.supply_placement_ok(board, state, *node)?;
                }
                Ok(())
            }
            Move::Sever(node) => {
                if Self::charges(state, state.to_move) == 0 {
                    return Err(RuleError::SeverUnavailable);
                }
                if *node as usize >= board.node_count() {
                    return Err(RuleError::NoSuchNode(*node));
                }
                let enemy = state.to_move.opponent();
                if state.occupant(*node) != Some(enemy) {
                    return Err(RuleError::CannotSever(*node));
                }
                if board.definition().origins_of(enemy).contains(node) {
                    return Err(RuleError::CannotSever(*node));
                }
                Ok(())
            }
            Move::CutEdge(e) => Err(RuleError::CannotCut(*e)),
            Move::Pass => {
                if self.territory || self.supply {
                    Ok(())
                } else {
                    Err(RuleError::PassUnavailable)
                }
            }
            Move::Swap => {
                if self.swap_available(state) {
                    Ok(())
                } else {
                    Err(RuleError::SwapUnavailable)
                }
            }
            Move::Resign => Ok(()),
        }
    }

    fn apply_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<GameState, RuleError> {
        self.validate_move(board, state, mv)?;
        let mover = state.to_move;
        let mut next = state.clone();
        next.move_log.push(*mv);
        next.ply += 1;
        if next.ply >= 2 {
            next.swap_used = true;
        }

        match mv {
            Move::Resign => {
                next.result = Some(GameResult::Win {
                    player: mover.opponent(),
                    reason: WinReason::Resignation,
                });
                return Ok(next);
            }
            Move::Swap => {
                // Colors are unchanged; seats swap outside the engine.
                return Ok(next);
            }
            Move::Pass => {
                next.consecutive_passes += 1;
                if next.consecutive_passes >= 2 {
                    next.result = Some(if self.supply {
                        self.score_supply(board, &next)
                    } else {
                        self.score_territory(board, &next)
                    });
                    return Ok(next);
                }
                next.to_move = mover.opponent();
                return Ok(next);
            }
            Move::CutEdge(e) => return Err(RuleError::CannotCut(*e)),
            Move::Place(node) => {
                next.occupancy[*node as usize] = Some(mover);
                next.consecutive_passes = 0;
            }
            Move::Sever(node) => {
                next.occupancy[*node as usize] = None;
                next.sever_charges[player_index(mover)] -= 1;
                next.consecutive_passes = 0;
            }
        }

        if self.supply {
            // Capture enemy groups whose supply this placement severed.
            let captured =
                capture_unsupplied(board, &mut next, mover.opponent(), self.supply_range);
            next.captures[player_index(mover)] += captured;
            // (Self-capture was excluded by validation.)
            next.position_hashes.push(position_hash(&next));
            let cap = board.node_count() as u32 * SUPPLY_PLY_CAP_FACTOR;
            if Self::board_full(&next) || next.ply >= cap {
                next.result = Some(self.score_supply(board, &next));
                return Ok(next);
            }
            next.to_move = mover.opponent();
            return Ok(next);
        }

        if self.territory {
            if Self::board_full(&next) {
                next.result = Some(self.score_territory(board, &next));
                return Ok(next);
            }
            next.to_move = mover.opponent();
            return Ok(next);
        }

        // Weave confirmation: if the opponent had a provisional weave, this
        // move was the response turn. (A sever may have just broken it.)
        if next.pending_weave == Some(mover.opponent()) {
            if self.has_weave(board, &next, mover.opponent()) {
                next.result = Some(GameResult::Win {
                    player: mover.opponent(),
                    reason: WinReason::RealmWeave,
                });
                return Ok(next);
            }
            next.pending_weave = None;
        }

        // New provisional weave by the mover?
        if self.has_weave(board, &next, mover) {
            next.pending_weave = Some(mover);
        } else if next.pending_weave == Some(mover) {
            next.pending_weave = None;
        }

        if Self::board_full(&next) && next.result.is_none() {
            // No confirmation turn is possible on a full board: a standing
            // weave that the opponent can never answer wins outright.
            // (With sever charges the opponent could still answer, but a
            // full board with charges left is not reachable in practice and
            // the simple rule keeps termination guaranteed.)
            if next.pending_weave == Some(mover) {
                next.result = Some(GameResult::Win {
                    player: mover,
                    reason: WinReason::RealmWeave,
                });
            } else {
                next.result = Some(GameResult::Draw);
            }
            return Ok(next);
        }

        next.to_move = mover.opponent();
        Ok(next)
    }

    fn evaluate(&self, _board: &BoardGraph, state: &GameState) -> Option<GameResult> {
        state.result
    }
}

/// Territory score: largest connected network + weave bonus.
pub fn territory_score(board: &BoardGraph, state: &GameState, player: Player) -> i32 {
    let components = player_components(board, state, player);
    let largest = components.iter().map(Vec::len).max().unwrap_or(0) as i32;
    let origins = board.definition().origins_of(player);
    let weave = components
        .iter()
        .any(|c| origins.iter().all(|o| c.binary_search(o).is_ok()));
    largest + if weave { TERRITORY_WEAVE_BONUS } else { 0 }
}

// ------------------------------------------------------------ weave-sever ---

/// Weave & Sever v2 — the design in docs/design-weave-sever-v2.md.
///
/// Each turn: Place a stone, Cut an edge (K scissors each, origin-adjacent
/// edges protected), or Pass. Win by confirmed Realm Weave over the living
/// graph, or instantly by strangling: the opponent's origins can never be
/// connected again even given every empty node. Self-strangling cuts are
/// illegal. Fallback scoring if the game stalls (two passes / full board):
/// most potentially-connectable origins, then most scissors, then draw.
pub struct WeaveSeverV2 {
    pub pie_rule: bool,
    /// 1 = classic v2 (first confirmed weave wins). >1 = weave-layers-v3:
    /// each confirmed weave scores a layer and petrifies its network; first
    /// to this many layers wins.
    pub layers_to_win: u8,
}

impl WeaveSeverV2 {
    fn swap_available(&self, state: &GameState) -> bool {
        self.pie_rule && !state.swap_used && state.ply == 1 && state.to_move == Player::Dark
    }

    /// Sanctum radius for this mode: wider on larger boards; a radius-2
    /// halo would swallow most of tiny hex19.
    fn sanctum_radius(&self, board: &BoardGraph) -> u32 {
        if self.layers_to_win > 1 && board.node_count() >= 37 * 3 {
            2
        } else {
            1
        }
    }

    /// Nodes within graph distance `radius` of any origin (any player).
    fn origin_zone(board: &BoardGraph, radius: u32) -> Vec<bool> {
        let def = board.definition();
        let mut zone = vec![false; board.node_count()];
        let mut queue = VecDeque::new();
        let mut dist = vec![u32::MAX; board.node_count()];
        for o in &def.origins {
            dist[o.node as usize] = 0;
            queue.push_back(o.node);
        }
        while let Some(cur) = queue.pop_front() {
            if dist[cur as usize] >= radius {
                continue;
            }
            for &nb in board.neighbors(cur) {
                if dist[nb as usize] == u32::MAX {
                    dist[nb as usize] = dist[cur as usize] + 1;
                    queue.push_back(nb);
                }
            }
        }
        for (n, d) in dist.iter().enumerate() {
            if *d <= radius {
                zone[n] = true;
            }
        }
        zone
    }

    /// Can edge `e` be cut at all (regardless of whose turn)?
    fn edge_cuttable(
        &self,
        board: &BoardGraph,
        state: &GameState,
        e: u32,
    ) -> Result<(), RuleError> {
        let def = board.definition();
        let Some(edge) = def.edges.get(e as usize) else {
            return Err(RuleError::CannotCut(e));
        };
        if state.cut_edges.contains(&e) {
            return Err(RuleError::CannotCut(e));
        }
        // Origin-adjacent edges (either player's) are protected. In the
        // layers game the protected halo widens to radius 1 around every
        // origin, and PORTAL edges are the world's skeleton — uncuttable.
        // Gates are fought over by occupation, not demolition; scissors
        // shape the terrain within realms.
        if self.layers_to_win > 1 {
            if edge.kind == crate::board::EdgeKind::Portal {
                return Err(RuleError::CannotCut(e));
            }
            let zone = Self::origin_zone(board, 1);
            if zone[edge.a as usize] || zone[edge.b as usize] {
                return Err(RuleError::CannotCut(e));
            }
        } else {
            let is_origin = |n: NodeId| def.origins.iter().any(|o| o.node == n);
            if is_origin(edge.a) || is_origin(edge.b) {
                return Err(RuleError::CannotCut(e));
            }
        }
        // Petrified world structure cannot be re-carved (v3).
        if state.is_petrified(edge.a) || state.is_petrified(edge.b) {
            return Err(RuleError::CannotCut(e));
        }
        Ok(())
    }

    /// v3: petrify `player`'s weave network. The network = the connected
    /// component (over live edges, own stones + origins) containing the
    /// origins. Origin-adjacent stones are REMOVED instead of petrified —
    /// origins must keep breathing room (design §1.2).
    fn petrify_weave(board: &BoardGraph, state: &mut GameState, player: Player) {
        let def = board.definition();
        let origins = def.origins_of(player);
        // Collect the network via BFS over own stones from the origins.
        let mut in_net = vec![false; board.node_count()];
        let mut queue = VecDeque::new();
        for &o in &origins {
            if !in_net[o as usize] {
                in_net[o as usize] = true;
                queue.push_back(o);
            }
        }
        while let Some(cur) = queue.pop_front() {
            for nb in board.live_neighbors(cur, &state.cut_edges) {
                if !in_net[nb as usize] && state.occupant(nb) == Some(player) {
                    in_net[nb as usize] = true;
                    queue.push_back(nb);
                }
            }
        }
        if state.petrified.len() < board.node_count() {
            state.petrified.resize(board.node_count(), None);
        }
        let origin_adjacent: Vec<bool> = {
            let mut adj = vec![false; board.node_count()];
            for o in &def.origins {
                for &nb in board.neighbors(o.node) {
                    adj[nb as usize] = true;
                }
            }
            adj
        };
        let is_origin: Vec<bool> = {
            let mut v = vec![false; board.node_count()];
            for o in &def.origins {
                v[o.node as usize] = true;
            }
            v
        };
        for n in 0..board.node_count() {
            if !in_net[n] || is_origin[n] || state.occupant(n as NodeId) != Some(player) {
                continue; // origins themselves stay origins
            }
            state.occupancy[n] = None;
            if !origin_adjacent[n] {
                state.petrified[n] = Some(player);
            }
            // origin-adjacent: removed entirely (breathing room)
        }
        // Layer scissors for both sides, capped.
        for sc in state.scissors.iter_mut() {
            *sc = (*sc + LAYER_SCISSORS).min(SCISSORS_CAP);
        }
    }

    /// Doom check for this mode: v2 = potential connectivity (stones
    /// block); v3 = permanent terrain only (cuts + petrification).
    fn doomed(&self, board: &BoardGraph, state: &GameState, player: Player) -> bool {
        if self.layers_to_win > 1 {
            !permanently_connected(board, state, player)
        } else {
            !potential_connected(board, state, player)
        }
    }

    fn board_full(state: &GameState) -> bool {
        state
            .occupancy
            .iter()
            .enumerate()
            .all(|(i, occ)| occ.is_some() || state.is_petrified(i as NodeId))
    }

    /// Fallback scoring per §2.5 of the design (v3: layers decide first).
    fn fallback_result(board: &BoardGraph, state: &GameState) -> GameResult {
        match state.layers[0].cmp(&state.layers[1]) {
            std::cmp::Ordering::Greater => {
                return GameResult::Win {
                    player: Player::Light,
                    reason: WinReason::RealmWeave,
                }
            }
            std::cmp::Ordering::Less => {
                return GameResult::Win {
                    player: Player::Dark,
                    reason: WinReason::RealmWeave,
                }
            }
            std::cmp::Ordering::Equal => {}
        }
        let l = potential_origin_groups(board, state, Player::Light);
        let d = potential_origin_groups(board, state, Player::Dark);
        match l.cmp(&d) {
            std::cmp::Ordering::Greater => GameResult::Win {
                player: Player::Light,
                reason: WinReason::Strangle,
            },
            std::cmp::Ordering::Less => GameResult::Win {
                player: Player::Dark,
                reason: WinReason::Strangle,
            },
            std::cmp::Ordering::Equal => match state.scissors[0].cmp(&state.scissors[1]) {
                std::cmp::Ordering::Greater => GameResult::Win {
                    player: Player::Light,
                    reason: WinReason::Strangle,
                },
                std::cmp::Ordering::Less => GameResult::Win {
                    player: Player::Dark,
                    reason: WinReason::Strangle,
                },
                std::cmp::Ordering::Equal => GameResult::Draw,
            },
        }
    }
}

impl RuleSet for WeaveSeverV2 {
    fn id(&self) -> &str {
        if self.layers_to_win > 1 {
            WEAVE_LAYERS_V3
        } else {
            WEAVE_SEVER_V2
        }
    }

    fn setup(&self, state: &mut GameState) {
        state.scissors = [SCISSORS, SCISSORS];
    }

    fn legal_moves(&self, board: &BoardGraph, state: &GameState) -> Vec<Move> {
        if state.is_finished() {
            return Vec::new();
        }
        // Origin sanctum: nodes near an ENEMY origin are unplaceable
        // (radius 1 in v2, radius 2 in the layers game).
        let def = board.definition();
        let enemy = state.to_move.opponent();
        let radius = self.sanctum_radius(board);
        let mut sanctum = vec![false; board.node_count()];
        for o in def.origins.iter().filter(|o| o.player == enemy) {
            let mut dist = vec![u32::MAX; board.node_count()];
            let mut queue = VecDeque::new();
            dist[o.node as usize] = 0;
            queue.push_back(o.node);
            while let Some(cur) = queue.pop_front() {
                if dist[cur as usize] >= radius {
                    continue;
                }
                for &nb in board.neighbors(cur) {
                    if dist[nb as usize] == u32::MAX {
                        dist[nb as usize] = dist[cur as usize] + 1;
                        sanctum[nb as usize] = true;
                        queue.push_back(nb);
                    }
                }
            }
        }
        let mut moves: Vec<Move> = (0..board.node_count() as NodeId)
            .filter(|&n| {
                state.occupant(n).is_none() && !sanctum[n as usize] && !state.is_petrified(n)
            })
            .map(Move::Place)
            .collect();
        if state.scissors[player_index(state.to_move)] > 0 {
            for e in 0..board.definition().edges.len() as u32 {
                if self.edge_cuttable(board, state, e).is_ok()
                    && !cut_self_strangles(board, state, state.to_move, e, self.layers_to_win > 1)
                {
                    moves.push(Move::CutEdge(e));
                }
            }
        }
        moves.push(Move::Pass);
        if self.swap_available(state) {
            moves.push(Move::Swap);
        }
        moves.push(Move::Resign);
        moves
    }

    fn validate_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<(), RuleError> {
        if state.is_finished() {
            return Err(RuleError::GameFinished);
        }
        match mv {
            Move::Place(node) => {
                if *node as usize >= board.node_count() {
                    return Err(RuleError::NoSuchNode(*node));
                }
                if state.occupant(*node).is_some() || state.is_petrified(*node) {
                    return Err(RuleError::Occupied(*node));
                }
                // Origin sanctum: you may not place near an ENEMY origin
                // (radius 1 in v2, radius 2 in the layers game). Origins
                // are low-degree corner nodes; without this a small
                // blockade strangles them outright and every game
                // degenerates into a blockade race.
                let def = board.definition();
                let enemy = state.to_move.opponent();
                let radius = self.sanctum_radius(board);
                for o in def.origins.iter().filter(|o| o.player == enemy) {
                    // BFS out to `radius` from the origin.
                    let mut dist = vec![u32::MAX; board.node_count()];
                    let mut queue = VecDeque::new();
                    dist[o.node as usize] = 0;
                    queue.push_back(o.node);
                    while let Some(cur) = queue.pop_front() {
                        if cur == *node && dist[cur as usize] <= radius {
                            return Err(RuleError::OriginSanctum(*node));
                        }
                        if dist[cur as usize] >= radius {
                            continue;
                        }
                        for &nb in board.neighbors(cur) {
                            if dist[nb as usize] == u32::MAX {
                                dist[nb as usize] = dist[cur as usize] + 1;
                                queue.push_back(nb);
                            }
                        }
                    }
                }
                Ok(())
            }
            Move::CutEdge(e) => {
                if state.scissors[player_index(state.to_move)] == 0 {
                    return Err(RuleError::NoScissors);
                }
                self.edge_cuttable(board, state, *e)?;
                if cut_self_strangles(board, state, state.to_move, *e, self.layers_to_win > 1) {
                    return Err(RuleError::SelfStrangle);
                }
                Ok(())
            }
            Move::Pass => Ok(()),
            Move::Swap => {
                if self.swap_available(state) {
                    Ok(())
                } else {
                    Err(RuleError::SwapUnavailable)
                }
            }
            Move::Sever(n) => Err(RuleError::CannotSever(*n)),
            Move::Resign => Ok(()),
        }
    }

    fn apply_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<GameState, RuleError> {
        self.validate_move(board, state, mv)?;
        let mover = state.to_move;
        let opp = mover.opponent();
        let mut next = state.clone();
        next.move_log.push(*mv);
        next.ply += 1;
        if next.ply >= 2 {
            next.swap_used = true;
        }

        match mv {
            Move::Resign => {
                next.result = Some(GameResult::Win {
                    player: opp,
                    reason: WinReason::Resignation,
                });
                return Ok(next);
            }
            Move::Swap => return Ok(next),
            Move::Pass => {
                next.consecutive_passes += 1;
                if next.consecutive_passes >= 2 {
                    next.result = Some(Self::fallback_result(board, &next));
                    return Ok(next);
                }
                // A pass forfeits the pending-weave response; confirmation
                // still runs below.
            }
            Move::Place(node) => {
                next.occupancy[*node as usize] = Some(mover);
                next.consecutive_passes = 0;
            }
            Move::CutEdge(e) => {
                next.cut_edges.push(*e);
                next.scissors[player_index(mover)] -= 1;
                next.consecutive_passes = 0;
            }
            Move::Sever(n) => return Err(RuleError::CannotSever(*n)),
        }

        // 1. Strangle check — death before life (design §2.3, edge case 4).
        //    A cut or a blocking placement may have doomed the opponent.
        if self.doomed(board, &next, opp) {
            next.result = Some(GameResult::Win {
                player: mover,
                reason: WinReason::Strangle,
            });
            return Ok(next);
        }

        // 2. Pending-weave confirmation (opponent's weave survived my turn?).
        if next.pending_weave == Some(opp) {
            if live_realm_weave(board, &next, opp) {
                next.layers[player_index(opp)] += 1;
                if next.layers[player_index(opp)] >= self.layers_to_win {
                    next.result = Some(GameResult::Win {
                        player: opp,
                        reason: WinReason::RealmWeave,
                    });
                    return Ok(next);
                }
                // v3: the confirmed weave petrifies into world structure
                // and play continues on the transformed board.
                Self::petrify_weave(board, &mut next, opp);
                next.pending_weave = None;
                // Petrification may have doomed someone: strangle checks.
                // Self-doom by petrify loses (design edge case 2); check
                // the petrifier FIRST so simultaneous doom favors the
                // non-scorer per that rule.
                if self.doomed(board, &next, opp) {
                    next.result = Some(GameResult::Win {
                        player: mover,
                        reason: WinReason::Strangle,
                    });
                    return Ok(next);
                }
                if self.doomed(board, &next, mover) {
                    next.result = Some(GameResult::Win {
                        player: opp,
                        reason: WinReason::Strangle,
                    });
                    return Ok(next);
                }
            } else {
                next.pending_weave = None;
            }
        }

        // 3. New provisional weave by the mover?
        if live_realm_weave(board, &next, mover) {
            next.pending_weave = Some(mover);
        } else if next.pending_weave == Some(mover) {
            next.pending_weave = None;
        }

        // 4a. Ply cap (v3): close the game and score it.
        if self.layers_to_win > 1 && next.ply >= V3_PLY_CAP && next.result.is_none() {
            next.result = Some(Self::fallback_result(board, &next));
            return Ok(next);
        }

        // 4. Full-board endgame (no response turn possible).
        if Self::board_full(&next) && next.result.is_none() {
            if next.pending_weave == Some(mover) {
                // A standing weave the opponent can never answer scores.
                next.layers[player_index(mover)] += 1;
            }
            next.result = Some(if next.layers[player_index(mover)] >= self.layers_to_win {
                GameResult::Win {
                    player: mover,
                    reason: WinReason::RealmWeave,
                }
            } else {
                Self::fallback_result(board, &next)
            });
            return Ok(next);
        }

        next.to_move = opp;
        Ok(next)
    }

    fn evaluate(&self, _board: &BoardGraph, state: &GameState) -> Option<GameResult> {
        state.result
    }
}

// ------------------------------------------------------------- trinity-y ---

/// Trinity Y (v4) — the whole rulebook:
///
///   1. On your turn, place a stone on any empty node (or use the pie swap).
///   2. A realm is WON by the player whose single group connects all three
///      of that realm's sides. Y theorem: a full triangle has exactly one
///      such player — no realm can end undecided.
///   3. Win the match by winning TWO of the three realms.
///
/// Nothing else. No origins, no scissors, no sanctums, no caps: the one
/// resource is tempo (each stone played in one realm is a stone not played
/// in the other two), and attack IS defense (blocking your opponent's Y is
/// building your own — the Y theorem again).
pub struct TrinityY {
    pub pie_rule: bool,
}

impl TrinityY {
    /// Triangle side length from the node count (3 * side*(side+1)/2).
    fn side_of(board: &BoardGraph) -> usize {
        let per_realm = board.node_count() / 3;
        // side*(side+1)/2 = per_realm
        (((((8 * per_realm + 1) as f64).sqrt() - 1.0) / 2.0).round()) as usize
    }

    /// Who (if anyone) has a Y in this realm: one group touching all
    /// three sides.
    pub fn realm_winner(board: &BoardGraph, state: &GameState, realm_index: usize) -> Option<Player> {
        let side = Self::side_of(board);
        let per_realm = board.node_count() / 3;
        let lo = (realm_index * per_realm) as NodeId;
        let hi = lo + per_realm as NodeId;
        for player in [Player::Light, Player::Dark] {
            let mut visited = vec![false; per_realm];
            for start in lo..hi {
                if state.occupant(start) != Some(player) || visited[(start - lo) as usize] {
                    continue;
                }
                let mut queue = VecDeque::new();
                visited[(start - lo) as usize] = true;
                queue.push_back(start);
                let mut touch = crate::boardgen::trinity_sides(side, start);
                while let Some(cur) = queue.pop_front() {
                    for &nb in board.neighbors(cur) {
                        if nb < lo || nb >= hi {
                            continue;
                        }
                        if !visited[(nb - lo) as usize] && state.occupant(nb) == Some(player) {
                            visited[(nb - lo) as usize] = true;
                            queue.push_back(nb);
                            touch |= crate::boardgen::trinity_sides(side, nb);
                        }
                    }
                }
                if touch == 7 {
                    return Some(player);
                }
            }
        }
        None
    }

    /// Current realm scores ([Light, Dark]).
    pub fn realm_scores(board: &BoardGraph, state: &GameState) -> [u8; 2] {
        let mut scores = [0u8; 2];
        for realm in 0..3 {
            if let Some(p) = Self::realm_winner(board, state, realm) {
                scores[player_index(p)] += 1;
            }
        }
        scores
    }

    fn swap_available(&self, state: &GameState) -> bool {
        self.pie_rule && !state.swap_used && state.ply == 1 && state.to_move == Player::Dark
    }
}

impl RuleSet for TrinityY {
    fn id(&self) -> &str {
        TRINITY_Y_V4
    }

    fn setup(&self, _state: &mut GameState) {}

    fn legal_moves(&self, board: &BoardGraph, state: &GameState) -> Vec<Move> {
        if state.is_finished() {
            return Vec::new();
        }
        let mut moves: Vec<Move> = (0..board.node_count() as NodeId)
            .filter(|&n| state.occupant(n).is_none())
            .map(Move::Place)
            .collect();
        if self.swap_available(state) {
            moves.push(Move::Swap);
        }
        moves.push(Move::Resign);
        moves
    }

    fn validate_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<(), RuleError> {
        if state.is_finished() {
            return Err(RuleError::GameFinished);
        }
        match mv {
            Move::Place(node) => {
                if *node as usize >= board.node_count() {
                    return Err(RuleError::NoSuchNode(*node));
                }
                if state.occupant(*node).is_some() {
                    return Err(RuleError::Occupied(*node));
                }
                Ok(())
            }
            Move::Swap => {
                if self.swap_available(state) {
                    Ok(())
                } else {
                    Err(RuleError::SwapUnavailable)
                }
            }
            Move::Resign => Ok(()),
            Move::Pass => Err(RuleError::PassUnavailable),
            Move::CutEdge(e) => Err(RuleError::CannotCut(*e)),
            Move::Sever(n) => Err(RuleError::CannotSever(*n)),
        }
    }

    fn apply_move(
        &self,
        board: &BoardGraph,
        state: &GameState,
        mv: &Move,
    ) -> Result<GameState, RuleError> {
        self.validate_move(board, state, mv)?;
        let mover = state.to_move;
        let mut next = state.clone();
        next.move_log.push(*mv);
        next.ply += 1;
        if next.ply >= 2 {
            next.swap_used = true;
        }
        match mv {
            Move::Resign => {
                next.result = Some(GameResult::Win {
                    player: mover.opponent(),
                    reason: WinReason::Resignation,
                });
                return Ok(next);
            }
            Move::Swap => return Ok(next),
            Move::Place(node) => {
                next.occupancy[*node as usize] = Some(mover);
            }
            _ => unreachable!("validated"),
        }
        // Track realm scores in `layers` (reuse: [Light, Dark] realms won).
        let scores = Self::realm_scores(board, &next);
        next.layers = scores;
        if scores[player_index(mover)] >= 2 {
            next.result = Some(GameResult::Win {
                player: mover,
                reason: WinReason::RealmWeave,
            });
        } else if scores[player_index(mover.opponent())] >= 2 {
            next.result = Some(GameResult::Win {
                player: mover.opponent(),
                reason: WinReason::RealmWeave,
            });
        }
        next.to_move = mover.opponent();
        Ok(next)
    }

    fn evaluate(&self, _board: &BoardGraph, state: &GameState) -> Option<GameResult> {
        state.result
    }
}

/// Are `player`'s origins potentially connectable: treating own stones AND
/// empty nodes as passable (enemy stones block) over the LIVE graph?
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
fn cut_self_strangles(
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

// ---------------------------------------------------------------- supply ---

/// Does the group containing `start` (owned by `player`) have a supply line:
/// a path through own stones and empty nodes to one of the player's origins?
/// Origins themselves always have supply.
///
/// `range`: if set, the path may cross at most that many EMPTY nodes (own
/// stones are free). 0/1-BFS over (node, empties-used) with monotone budget.
pub fn group_has_supply(
    board: &BoardGraph,
    state: &GameState,
    player: Player,
    start: NodeId,
    range: Option<u32>,
) -> bool {
    if state.occupant(start) != Some(player) {
        return true; // vacuously
    }
    let origins = board.definition().origins_of(player);
    let budget = range.unwrap_or(u32::MAX);
    // Deque BFS with 0/1 weights: own stone edges cost 0, empty nodes 1.
    // dist[n] = min empties used to reach n.
    let mut dist = vec![u32::MAX; board.node_count()];
    let mut deque = VecDeque::new();
    dist[start as usize] = 0;
    deque.push_back(start);
    while let Some(cur) = deque.pop_front() {
        if origins.contains(&cur) {
            return true;
        }
        let d = dist[cur as usize];
        for &next in board.neighbors(cur) {
            let cost = match state.occupant(next) {
                Some(p) if p == player => 0,
                None => 1,
                _ => continue,
            };
            let nd = d + cost;
            if nd <= budget && nd < dist[next as usize] {
                dist[next as usize] = nd;
                if cost == 0 {
                    deque.push_front(next);
                } else {
                    deque.push_back(next);
                }
            }
        }
    }
    false
}

/// Remove all of `player`'s groups that have no supply line. Origins are
/// never removed (they are their own supply). Returns removed stone count.
pub fn capture_unsupplied(
    board: &BoardGraph,
    state: &mut GameState,
    player: Player,
    range: Option<u32>,
) -> u32 {
    let origins = board.definition().origins_of(player);
    let mut checked = vec![false; board.node_count()];
    let mut to_remove: Vec<NodeId> = Vec::new();
    for node in 0..board.node_count() as NodeId {
        if state.occupant(node) != Some(player) || checked[node as usize] {
            continue;
        }
        let group = connected_component(board, state, player, node);
        for &g in &group {
            checked[g as usize] = true;
        }
        // A group containing an origin always has supply.
        if group.iter().any(|g| origins.contains(g)) {
            continue;
        }
        if !group_has_supply(board, state, player, node, range) {
            to_remove.extend(group.iter().filter(|g| !origins.contains(g)));
        }
    }
    let removed = to_remove.len() as u32;
    for node in to_remove {
        state.occupancy[node as usize] = None;
    }
    removed
}

/// FNV-1a hash of the occupancy + side to move (positional superko key).
pub fn position_hash(state: &GameState) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |byte: u8| {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for occ in &state.occupancy {
        mix(match occ {
            None => 0,
            Some(Player::Light) => 1,
            Some(Player::Dark) => 2,
        });
    }
    mix(match state.to_move {
        Player::Light => 3,
        Player::Dark => 4,
    });
    hash
}

/// Detailed supply-mode score for one player (UI/tooling).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupplyScore {
    pub stones: i32,
    pub territory: i32,
    pub weave_bonus: i32,
    /// Komi in half-points (only nonzero for Dark).
    pub komi_half: i32,
}

impl SupplyScore {
    /// Total in half-points (comparable across players).
    pub fn total_half(&self) -> i32 {
        2 * (self.stones + self.territory + self.weave_bonus) + self.komi_half
    }

    /// Human-readable total, e.g. "37.5".
    pub fn display(&self) -> String {
        let h = self.total_half();
        if h % 2 == 0 {
            format!("{}", h / 2)
        } else {
            format!("{}.5", h / 2)
        }
    }
}

/// Full score breakdown for one player under supply rules.
pub fn supply_score(board: &BoardGraph, state: &GameState, player: Player) -> SupplyScore {
    let stones = state
        .occupancy
        .iter()
        .filter(|o| **o == Some(player))
        .count() as i32;
    let territory = supply_territory_nodes(board, state, player).len() as i32;
    let weave_bonus = if has_realm_weave(board, state, player) {
        SUPPLY_WEAVE_BONUS
    } else {
        0
    };
    let komi_half = if player == Player::Dark {
        SUPPLY_KOMI_HALF
    } else {
        0
    };
    SupplyScore {
        stones,
        territory,
        weave_bonus,
        komi_half,
    }
}

/// Empty nodes in regions bordered exclusively by `player` (for scoring and
/// territory rendering).
pub fn supply_territory_nodes(
    board: &BoardGraph,
    state: &GameState,
    player: Player,
) -> Vec<NodeId> {
    let mut mine = Vec::new();
    let mut visited = vec![false; board.node_count()];
    for node in 0..board.node_count() as NodeId {
        if state.occupant(node).is_some() || visited[node as usize] {
            continue;
        }
        let mut region = Vec::new();
        let mut borders_light = false;
        let mut borders_dark = false;
        let mut queue = VecDeque::new();
        visited[node as usize] = true;
        queue.push_back(node);
        while let Some(cur) = queue.pop_front() {
            region.push(cur);
            for &next in board.neighbors(cur) {
                match state.occupant(next) {
                    None => {
                        if !visited[next as usize] {
                            visited[next as usize] = true;
                            queue.push_back(next);
                        }
                    }
                    Some(Player::Light) => borders_light = true,
                    Some(Player::Dark) => borders_dark = true,
                }
            }
        }
        let owned = match player {
            Player::Light => borders_light && !borders_dark,
            Player::Dark => borders_dark && !borders_light,
        };
        if owned {
            mine.extend(region);
        }
    }
    mine
}

/// Connected component of `player`'s network containing `start`.
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
            let (u, ei) = prev[v].expect("path");
            let rev = graph[u][ei].2;
            graph[u][ei].1 -= 1;
            graph[v][rev].1 += 1;
            v = u;
        }
        flow += 1;
    }
    flow
}
