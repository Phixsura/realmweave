//! Interactive local two-player terminal game.

#![allow(clippy::unwrap_used, clippy::expect_used)] // offline tooling: fail fast is correct
use std::io::{BufRead, Write};

use realmweave_core::{BoardGraph, Game, GameConfig, Move, NodeId, Realm};

use crate::ascii;

pub fn run_interactive(graph: BoardGraph, pie: bool, ruleset: &str) -> Result<(), String> {
    let config = GameConfig::new(graph.definition().id.clone())
        .with_pie_rule(pie)
        .with_ruleset(ruleset);
    let mut game = Game::new(graph, config).map_err(|e| e.to_string())?;
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();

    println!(
        "Realmweave — local two-player game on {}",
        game.board().definition().id
    );
    println!("enter moves as a node id (e.g. `42`) or `<realm> q,r` (e.g. `mortal 1,0`);");
    println!("commands: swap, resign, undo, moves, quit; `pass`, `sever <node>`,");
    println!("  and `cut <realm> q,r / <realm> q,r` (weave-sever: e.g. `cut m 0,0 / m 1,0`)");

    loop {
        println!("\n{}", ascii::render(&game));
        if let Some(result) = game.result() {
            println!("game over: {result:?}");
            let record = game.record();
            let json = serde_json::to_string_pretty(&record).unwrap();
            let path = "realmweave-game.json";
            if std::fs::write(path, json + "\n").is_ok() {
                println!("game record written to {path}");
            }
            return Ok(());
        }
        print!("{} to move > ", game.to_move().name());
        std::io::stdout().flush().ok();
        let Some(Ok(line)) = lines.next() else {
            println!("\ninput closed; exiting");
            return Ok(());
        };
        let input = line.trim().to_lowercase();
        let result = match input.as_str() {
            "" => continue,
            "quit" | "exit" => return Ok(()),
            "swap" => game.swap_sides().map(|_| ()).map_err(|e| e.to_string()),
            "pass" => game.play(Move::Pass).map(|_| ()).map_err(|e| e.to_string()),
            "resign" => game
                .play(Move::Resign)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "undo" => game.undo().map(|_| ()).map_err(|e| e.to_string()),
            "moves" => {
                let moves = game.legal_moves();
                let placements: Vec<NodeId> = moves
                    .iter()
                    .filter_map(|m| match m {
                        Move::Place(n) => Some(*n),
                        _ => None,
                    })
                    .collect();
                println!("{} legal placements: {placements:?}", placements.len());
                if moves.contains(&Move::Swap) {
                    println!("swap is available (pie rule)");
                }
                continue;
            }
            other if other.starts_with("cut ") => {
                let spec = other.trim_start_matches("cut ").trim();
                match parse_edge(&game, spec) {
                    Ok(edge) => game
                        .play(Move::CutEdge(edge))
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                }
            }
            other if other.starts_with("sever ") => {
                match parse_node(&game, other.trim_start_matches("sever ").trim()) {
                    Ok(node) => game
                        .play(Move::Sever(node))
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                }
            }
            other => match parse_node(&game, other) {
                Ok(node) => game
                    .play(Move::Place(node))
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            },
        };
        if let Err(e) = result {
            println!("illegal: {e}");
        }
    }
}

/// Parse "realm q,r / realm q,r" into an edge index.
fn parse_edge(game: &Game, input: &str) -> Result<u32, String> {
    let (left, right) = input
        .split_once('/')
        .ok_or_else(|| format!("cut needs two endpoints separated by `/`, got `{input}`"))?;
    let a = parse_node(game, left.trim())?;
    let b = parse_node(game, right.trim())?;
    game.board()
        .definition()
        .edges
        .iter()
        .position(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
        .map(|i| i as u32)
        .ok_or_else(|| "no edge between those nodes".to_string())
}

fn parse_node(game: &Game, input: &str) -> Result<NodeId, String> {
    if let Ok(id) = input.parse::<NodeId>() {
        return Ok(id);
    }
    let (realm_str, coords) = input
        .split_once(' ')
        .ok_or_else(|| format!("cannot parse move `{input}`"))?;
    let realm = match realm_str {
        "heaven" | "h" => Realm::Heaven,
        "mortal" | "m" => Realm::Mortal,
        "underworld" | "u" => Realm::Underworld,
        other => return Err(format!("unknown realm `{other}`")),
    };
    let (q, r) = coords
        .split_once(',')
        .ok_or_else(|| format!("cannot parse coordinate `{coords}`"))?;
    let q: i32 = q.trim().parse().map_err(|_| format!("bad q `{q}`"))?;
    let r: i32 = r.trim().parse().map_err(|_| format!("bad r `{r}`"))?;
    game.board()
        .axial_index()
        .get(&(realm, [q, r]))
        .copied()
        .ok_or_else(|| format!("no node at {realm:?} {q},{r}"))
}
