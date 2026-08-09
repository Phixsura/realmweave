//! Realmweave simulation & balance tooling: self-play, board comparison,
//! and graph fairness analysis.

mod annotate;
mod bots;
mod fairness;
mod mcts;
mod stats;
mod territory_bot;

use std::path::PathBuf;
use std::process::ExitCode;

use bots::{Bot, GreedyBot, RandomBot};
use clap::{Parser, Subcommand};
use mcts::MctsBot;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use realmweave_core::{
    validate_board, BoardDefinition, BoardGraph, Game, GameConfig, GameResult, Move, Player,
};
use stats::BatchStats;
use territory_bot::TerritoryBot;

#[derive(Parser)]
#[command(
    name = "realmweave-sim",
    about = "Self-play simulation and balance tooling"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run bot self-play on one board and print balance statistics.
    Selfplay {
        #[arg(long)]
        board: PathBuf,
        #[arg(long, default_value_t = 100)]
        games: u32,
        /// Bot for both sides: random | greedy | mcts | territory
        /// (override one side with --light-bot / --dark-bot).
        #[arg(long, default_value = "greedy")]
        bot: String,
        #[arg(long)]
        light_bot: Option<String>,
        #[arg(long)]
        dark_bot: Option<String>,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Enable the pie rule: the second player swaps seats when the
        /// estimated first-move advantage exceeds 50%.
        #[arg(long)]
        pie: bool,
        /// MCTS playouts per move (bot=mcts only).
        #[arg(long, default_value_t = 400)]
        playouts: u32,
        /// Ruleset id (three-realms-v1 | three-realms-doubleweave-v1 |
        /// three-realms-sever-v1 | three-realms-territory-v1).
        #[arg(long, default_value = "three-realms-v1")]
        ruleset: String,
        /// Save the first game's record (replayable JSON) to this path.
        #[arg(long)]
        record: Option<PathBuf>,
    },
    /// Run identical bot pairings on two boards and print both summaries.
    Compare {
        #[arg(long)]
        board_a: PathBuf,
        #[arg(long)]
        board_b: PathBuf,
        #[arg(long, default_value_t = 100)]
        games: u32,
        #[arg(long, default_value = "greedy")]
        bot: String,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value_t = 400)]
        playouts: u32,
    },
    /// Graph-level fairness analysis of a board definition.
    Fairness {
        #[arg(long)]
        board: PathBuf,
    },
    /// Generate per-move commentary JSON for a recorded game.
    Annotate {
        /// Game record file.
        record: PathBuf,
        #[arg(long)]
        board: PathBuf,
        /// Output annotations JSON (default: <record>.notes.json).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn load(path: &PathBuf) -> Result<BoardGraph, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let def: BoardDefinition =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    validate_board(&def).map_err(|e| e.to_string())
}

fn make_bot(kind: &str, seed: u64, playouts: u32) -> Result<Box<dyn Bot>, String> {
    match kind {
        "random" => Ok(Box::new(RandomBot {
            rng: StdRng::seed_from_u64(seed),
        })),
        "greedy" => Ok(Box::new(GreedyBot {
            rng: StdRng::seed_from_u64(seed),
            epsilon: 0.1,
        })),
        "mcts" => Ok(Box::new(MctsBot {
            rng: StdRng::seed_from_u64(seed),
            playouts,
            c: 1.2,
        })),
        "territory" => Ok(Box::new(TerritoryBot {
            rng: StdRng::seed_from_u64(seed),
            reply_width: 8,
        })),
        other => Err(format!(
            "unknown bot {other}; use random, greedy, mcts, or territory"
        )),
    }
}

/// Estimate Light's win probability with uniform random rollouts from the
/// current position. Used for the pie-rule swap decision.
fn estimate_light_winrate(game: &Game, rollouts: u32, rng: &mut StdRng) -> f64 {
    let mut light_score = 0.0;
    for _ in 0..rollouts {
        let board = BoardGraph::new(game.board().definition().clone()).expect("board");
        let mut sim = Game::replay(board, game.config().clone(), &game.state().move_log)
            .expect("replay clone");
        loop {
            if let Some(result) = sim.result() {
                light_score += match result {
                    GameResult::Win {
                        player: Player::Light,
                        ..
                    } => 1.0,
                    GameResult::Draw => 0.5,
                    _ => 0.0,
                };
                break;
            }
            let placements: Vec<Move> = sim
                .legal_moves()
                .into_iter()
                .filter(|m| matches!(m, Move::Place(_) | Move::Sever(_)))
                .collect();
            let Some(&mv) = placements.choose(rng) else {
                break;
            };
            let _ = sim.play(mv);
        }
    }
    light_score / rollouts as f64
}

struct BatchOptions<'a> {
    games: u32,
    bot_kind: &'a str,
    light_bot: Option<&'a str>,
    dark_bot: Option<&'a str>,
    seed: u64,
    pie: bool,
    playouts: u32,
    ruleset: &'a str,
    record: Option<&'a PathBuf>,
}

fn run_batch(board: &BoardGraph, opts: &BatchOptions) -> Result<BatchStats, String> {
    let mut stats = BatchStats::default();
    for g in 0..opts.games {
        let mut light = make_bot(
            opts.light_bot.unwrap_or(opts.bot_kind),
            opts.seed.wrapping_add(g as u64 * 2),
            opts.playouts,
        )?;
        let mut dark = make_bot(
            opts.dark_bot.unwrap_or(opts.bot_kind),
            opts.seed.wrapping_add(g as u64 * 2 + 1),
            opts.playouts,
        )?;
        let config = GameConfig::new(board.definition().id.clone())
            .with_pie_rule(opts.pie)
            .with_ruleset(opts.ruleset);
        let fresh = BoardGraph::new(board.definition().clone()).map_err(|e| e.to_string())?;
        let mut game = Game::new(fresh, config).map_err(|e| e.to_string())?;

        // Person A starts as Light; a pie swap exchanges the persons' seats.
        let mut person_a_plays = Player::Light;
        let mut swapped = false;
        let mut pie_rng = StdRng::seed_from_u64(opts.seed.wrapping_add(g as u64).wrapping_mul(31));

        while game.result().is_none() {
            // Pie decision on Dark's first response.
            if opts.pie
                && !swapped
                && game.state().ply == 1
                && game.legal_moves().contains(&Move::Swap)
            {
                let light_wr = estimate_light_winrate(&game, 40, &mut pie_rng);
                if light_wr > 0.5 {
                    game.play(Move::Swap).map_err(|e| e.to_string())?;
                    person_a_plays = Player::Dark; // persons exchange seats
                    swapped = true;
                    continue;
                }
                swapped = true; // declined; option gone
            }
            let bot = if game.to_move() == Player::Light {
                &mut light
            } else {
                &mut dark
            };
            let Some(mv) = bot.choose(&game) else { break };
            game.play(mv).map_err(|e| e.to_string())?;
        }
        if g == 0 {
            if let Some(path) = opts.record {
                let json =
                    serde_json::to_string_pretty(&game.record()).map_err(|e| e.to_string())?;
                std::fs::write(path, json + "\n").map_err(|e| e.to_string())?;
                eprintln!(
                    "saved game 1 record ({} moves) to {}",
                    game.state().move_log.len(),
                    path.display()
                );
            }
        }
        stats.record_with_persons(&game, person_a_plays);
    }
    Ok(stats)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Selfplay {
            board,
            games,
            bot,
            seed,
            pie,
            playouts,
            ruleset,
            record,
            light_bot,
            dark_bot,
        } => {
            let graph = load(&board)?;
            let stats = run_batch(
                &graph,
                &BatchOptions {
                    games,
                    bot_kind: &bot,
                    light_bot: light_bot.as_deref(),
                    dark_bot: dark_bot.as_deref(),
                    seed,
                    pie,
                    playouts,
                    ruleset: &ruleset,
                    record: record.as_ref(),
                },
            )?;
            println!(
                "{}",
                stats.summary(&format!(
                    "selfplay {} × {games} games, bot={bot}, seed={seed}, pie={pie}, rules={ruleset}",
                    graph.definition().id
                ))
            );
            Ok(())
        }
        Command::Compare {
            board_a,
            board_b,
            games,
            bot,
            seed,
            playouts,
        } => {
            let a = load(&board_a)?;
            let b = load(&board_b)?;
            let opts = BatchOptions {
                games,
                bot_kind: &bot,
                seed,
                pie: false,
                playouts,
                ruleset: realmweave_core::THREE_REALMS_V1,
                record: None,
                light_bot: None,
                dark_bot: None,
            };
            let stats_a = run_batch(&a, &opts)?;
            let stats_b = run_batch(&b, &opts)?;
            println!(
                "{}\n",
                stats_a.summary(&format!("[A] {}", a.definition().id))
            );
            println!("{}", stats_b.summary(&format!("[B] {}", b.definition().id)));
            Ok(())
        }
        Command::Fairness { board } => {
            let graph = load(&board)?;
            print!("{}", fairness::report(&graph));
            Ok(())
        }
        Command::Annotate {
            record,
            board,
            output,
        } => {
            let graph = load(&board)?;
            let text = std::fs::read_to_string(&record)
                .map_err(|e| format!("{}: {e}", record.display()))?;
            let rec: realmweave_core::GameRecord =
                serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let notes = annotate::annotate(graph, rec.config, &rec.moves);
            let out_path = output.unwrap_or_else(|| {
                let mut p = record.clone();
                p.set_extension("notes.json");
                p
            });
            let json = serde_json::to_string_pretty(&notes).map_err(|e| e.to_string())?;
            std::fs::write(&out_path, json + "\n").map_err(|e| e.to_string())?;
            eprintln!(
                "wrote {} annotations to {}",
                notes.len(),
                out_path.display()
            );
            Ok(())
        }
    }
}
