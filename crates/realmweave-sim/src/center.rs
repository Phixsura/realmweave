//! Center-advantage measurement for triforce boards.
//!
//! The one documented flaw of flat-Y games is center dominance. This tool
//! quantifies it for a given board: forced-opening win rates (heart center
//! vs corner belly vs big-side midpoint) plus free-game heart-occupancy
//! statistics. Criteria (fixed in docs/design-triforce-v5.md):
//!   - heart-opening win rate exceeds corner-opening by >= 15pp, or
//!   - free games put >= 60% of the first 10 moves in the heart
//!     (heart area share is only ~22%),
//!
//! => "center dominance confirmed".

#![allow(clippy::unwrap_used, clippy::expect_used)] // offline tooling: fail fast is correct
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, GameResult, Move, NodeId, Player};

/// The node of `region` nearest to that region's own centroid (cartesian
/// positions, so pierced boards probe the true visual center).
fn region_centroid_node(board: &BoardGraph, side: usize, region: usize) -> NodeId {
    let def = board.definition();
    let members: Vec<&realmweave_core::Node> = def
        .nodes
        .iter()
        .filter(|n| boardgen::triforce_region(def, side, n.id) == region)
        .collect();
    let cx = members.iter().map(|n| n.position[0] as f64).sum::<f64>() / members.len() as f64;
    let cz = members.iter().map(|n| n.position[2] as f64).sum::<f64>() / members.len() as f64;
    members
        .iter()
        .min_by(|a, b| {
            let da = (a.position[0] as f64 - cx).powi(2) + (a.position[2] as f64 - cz).powi(2);
            let db = (b.position[0] as f64 - cx).powi(2) + (b.position[2] as f64 - cz).powi(2);
            da.partial_cmp(&db).expect("finite distances")
        })
        .expect("regions are non-empty")
        .id
}

/// The three probe openings: heart centroid, corner-realm centroid
/// (Heaven's belly), and the bottom big-side midpoint.
fn probe_openings(board: &BoardGraph, side: usize) -> [(String, NodeId); 3] {
    let def = board.definition();
    let heart = region_centroid_node(board, side, 3);
    let corner = region_centroid_node(board, side, 0);
    // bottom-row middle: on the big side, far from every corner
    let edge = def
        .nodes
        .iter()
        .filter(|n| n.axial.expect("axial")[0] as usize == side - 1)
        .min_by_key(|n| (2 * n.axial.expect("axial")[1] as i64 - (side as i64 - 1)).abs())
        .expect("bottom row exists")
        .id;
    [
        ("heart-center".into(), heart),
        ("corner-belly".into(), corner),
        ("side-mid".into(), edge),
    ]
}

fn fresh_game(board: &BoardGraph, ruleset: &str) -> Game {
    let config = GameConfig::new(board.definition().id.clone())
        .with_pie_rule(false)
        .with_ruleset(ruleset);
    let fresh = BoardGraph::new(board.definition().clone()).expect("board clone");
    Game::new(fresh, config).expect("valid game")
}

/// Play one game to the end with the engine bot on both sides. Returns the
/// finished game.
fn play_out(mut game: Game, playouts: u32, seed: u64) -> Game {
    let budget = realmweave_bot::mcts::MctsConfig {
        playouts,
        ..Default::default()
    };
    let mut ply = 0u64;
    while game.result().is_none() {
        let Some(mv) =
            realmweave_bot::choose_move_with_budget(&game, seed.wrapping_add(ply), budget)
        else {
            break;
        };
        game.play(mv).expect("bot moves are validated");
        ply += 1;
    }
    game
}

/// The winner's side-spanning group (the winning weave), if any.
fn winning_group(game: &Game, side: usize) -> Option<Vec<NodeId>> {
    let GameResult::Win { player, .. } = game.result()? else {
        return None;
    };
    game.player_components(player).into_iter().find(|comp| {
        let mask = comp.iter().fold(0u8, |m, &n| {
            m | boardgen::triforce_sides(game.board().definition(), side, n)
        });
        mask == 7
    })
}

pub struct CenterReport {
    lines: Vec<String>,
}

impl std::fmt::Display for CenterReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for l in &self.lines {
            writeln!(f, "{l}")?;
        }
        Ok(())
    }
}

pub fn report(board: &BoardGraph, games: u32, playouts: u32, seed: u64) -> CenterReport {
    let side = boardgen::tf_side_len(board.definition());
    let heart_nodes = (0..board.node_count() as NodeId)
        .filter(|&n| boardgen::triforce_region(board.definition(), side, n) == 3)
        .count();
    let mut lines = vec![
        format!(
            "center probe: {} (side {side}, {} nodes, heart {} = {:.0}%), \
             {games} games/opening, {playouts} playouts, seed {seed}",
            board.definition().id,
            board.node_count(),
            heart_nodes,
            100.0 * heart_nodes as f64 / board.node_count() as f64
        ),
        String::new(),
    ];

    // --- forced openings ---
    let mut rates: Vec<(String, f64)> = Vec::new();
    for (name, open) in probe_openings(board, side) {
        let mut first_wins = 0u32;
        let mut lens = Vec::new();
        for g in 0..games {
            let mut game = fresh_game(board, realmweave_core::TRIFORCE_V5);
            game.play(Move::Place(open)).expect("probe opening legal");
            let done = play_out(game, playouts, seed.wrapping_add(1 + g as u64 * 977));
            if let Some(GameResult::Win { player, .. }) = done.result() {
                if player == Player::Light {
                    first_wins += 1;
                }
            }
            lens.push(done.state().move_log.len());
        }
        let rate = first_wins as f64 / games as f64;
        let avg_len = lens.iter().sum::<usize>() as f64 / lens.len() as f64;
        lines.push(format!(
            "  {name:<13} opening node {open:>3}: first-player wins {first_wins}/{games} \
             ({:.0}%), avg length {avg_len:.0}",
            100.0 * rate
        ));
        rates.push((name, rate));
    }
    let heart_rate = rates[0].1;
    let corner_rate = rates[1].1;
    let gap_pp = 100.0 * (heart_rate - corner_rate);

    // --- free games: heart occupancy ---
    let mut early10_heart = 0usize;
    let mut early10_total = 0usize;
    let mut early20_heart = 0usize;
    let mut early20_total = 0usize;
    let mut win_heart = 0usize;
    let mut win_total = 0usize;
    for g in 0..games {
        let game = fresh_game(board, realmweave_core::TRIFORCE_V5);
        let done = play_out(game, playouts, seed.wrapping_add(9000 + g as u64 * 977));
        for (i, mv) in done.state().move_log.iter().enumerate() {
            let Move::Place(n) = mv else { continue };
            let in_heart = boardgen::triforce_region(board.definition(), side, *n) == 3;
            if i < 10 {
                early10_total += 1;
                early10_heart += in_heart as usize;
            }
            if i < 20 {
                early20_total += 1;
                early20_heart += in_heart as usize;
            }
        }
        if let Some(group) = winning_group(&done, side) {
            win_total += group.len();
            win_heart += group
                .iter()
                .filter(|&&n| boardgen::triforce_region(board.definition(), side, n) == 3)
                .count();
        }
    }
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * a as f64 / b as f64
        }
    };
    lines.push(String::new());
    lines.push(format!(
        "  free games:   heart share of first 10 moves {:.0}%  (first 20: {:.0}%)",
        pct(early10_heart, early10_total),
        pct(early20_heart, early20_total)
    ));
    lines.push(format!(
        "                heart share of winning weaves  {:.0}%",
        pct(win_heart, win_total)
    ));
    lines.push(String::new());

    // --- verdict against the documented criteria ---
    let crit_gap = gap_pp >= 15.0;
    let crit_occ = pct(early10_heart, early10_total) >= 60.0;
    lines.push(format!(
        "  criteria: heart-vs-corner gap {gap_pp:+.0}pp (threshold +15pp) -> {}",
        if crit_gap { "EXCEEDED" } else { "ok" }
    ));
    lines.push(format!(
        "            first-10 heart share {:.0}% (threshold 60%) -> {}",
        pct(early10_heart, early10_total),
        if crit_occ { "EXCEEDED" } else { "ok" }
    ));
    lines.push(format!(
        "  verdict: center dominance {}",
        if crit_gap || crit_occ {
            "CONFIRMED — consider the v5p pierced board"
        } else {
            "not confirmed at this strength"
        }
    ));
    CenterReport { lines }
}
