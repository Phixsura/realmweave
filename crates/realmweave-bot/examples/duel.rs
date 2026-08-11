//! Engine-vs-engine exhibition: same seed on the solid and pierced boards.
//! Usage: duel <board-id> <seed> <playouts> <out.json>
#![allow(clippy::unwrap_used, clippy::expect_used)] // offline example: fail fast
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (board_id, seed, playouts, out) = (
        args[1].as_str(),
        args[2].parse::<u64>().unwrap(),
        args[3].parse::<u32>().unwrap(),
        args[4].as_str(),
    );
    use realmweave_core::{boardgen, BoardGraph, Game, GameConfig};
    let def = boardgen::resolve(board_id).unwrap();
    let b = BoardGraph::new(def).unwrap();
    let cfg = GameConfig::new(b.definition().id.clone())
        .with_ruleset("triforce-v5")
        .with_pie_rule(true);
    let mut g = Game::new(b, cfg).unwrap();
    let budget = realmweave_bot::mcts::MctsConfig { playouts, c: 0.9 };
    let t = std::time::Instant::now();
    let mut ply = 0u64;
    while g.result().is_none() {
        let Some(mv) = realmweave_bot::choose_move_with_budget(&g, seed ^ ply, budget) else {
            break;
        };
        g.play(mv).unwrap();
        ply += 1;
    }
    let rec = serde_json::to_string_pretty(&g.record()).unwrap();
    std::fs::write(out, rec + "\n").unwrap();
    println!(
        "{board_id}: {:?} in {} moves ({:.0}s), swap={}",
        g.result().unwrap(),
        g.state().move_log.len(),
        t.elapsed().as_secs_f32(),
        g.state()
            .move_log
            .get(1)
            .map(|m| matches!(m, realmweave_core::Move::Swap))
            .unwrap_or(false)
    );
}
