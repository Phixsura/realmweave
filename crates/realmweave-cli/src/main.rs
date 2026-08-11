//! Realmweave debug/replay/topology tooling and local two-player play.

#![allow(clippy::unwrap_used, clippy::expect_used)] // offline tooling: fail fast is correct
mod ascii;
mod play;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use realmweave_core::boardgen::{self, HexBoardSpec, PortalSpec};
use realmweave_core::{validate_board, BoardDefinition, BoardGraph, Game, GameRecord};

#[derive(Parser)]
#[command(name = "realmweave", about = "Realmweave board & game tooling")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a standard three-realm hex board definition.
    GenBoard {
        /// Nodes per realm: 19, 37, 61, 91, or 127 (hex boards).
        #[arg(long, conflicts_with_all = ["trinity", "triforce"])]
        size: Option<usize>,
        /// Generate a trinity (triangular side-goal) board with this side
        /// length instead of a hex board.
        #[arg(long, conflicts_with = "triforce")]
        trinity: Option<usize>,
        /// Generate a triforce (merged-triangle flagship) board with this
        /// even side length.
        #[arg(long)]
        triforce: Option<usize>,
        /// With --triforce: pierce the weave-heart (v5p variant, six nodes
        /// removed; even side 22..=40).
        #[arg(long, requires = "triforce")]
        pierced: bool,
        /// Portal layout.
        #[arg(long, default_value = "inner6-outer6")]
        portals: String,
        /// Output file (JSON). Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Validate one or more board definition files.
    Validate { files: Vec<PathBuf> },
    /// Play a local two-player game in the terminal.
    Play {
        /// Board definition file, or a generated board id
        /// (hex19/37/61/91/127-v1, tri4..26-v4, tf8..40-v5, hex61-s123).
        #[arg(long)]
        board: String,
        /// Enable the pie (swap) rule.
        #[arg(long)]
        pie: bool,
        /// Ruleset id (trinity-y-v4 | weave-layers-v3 | weave-sever-v2 |
        /// three-realms-v1 | three-realms-sever-v1).
        #[arg(long, default_value = "trinity-y-v4")]
        ruleset: String,
    },
    /// Step through a recorded game (GameRecord JSON).
    Replay {
        /// Game record file.
        record: PathBuf,
        /// Board definition file (must match record's board id).
        #[arg(long)]
        board: PathBuf,
        /// Print every intermediate position, not just the final one.
        #[arg(long)]
        step: bool,
    },
}

fn load_board(path: &PathBuf) -> Result<BoardDefinition, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
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
        Command::GenBoard {
            size,
            trinity,
            triforce,
            pierced,
            portals,
            output,
        } => {
            if let Some(side) = triforce {
                let def = if pierced {
                    boardgen::generate_triforce_pierced(side).ok_or_else(|| {
                        format!("unsupported pierced side {side}; use even 22..=40")
                    })?
                } else {
                    boardgen::generate_triforce(side).ok_or_else(|| {
                        format!("unsupported triforce side {side}; use even 8..=40")
                    })?
                };
                validate_board(&def)
                    .map_err(|e| format!("generated board failed validation: {e}"))?;
                let json = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
                match output {
                    Some(path) => {
                        std::fs::write(&path, json + "\n")
                            .map_err(|e| format!("{}: {e}", path.display()))?;
                        eprintln!(
                            "wrote {} ({} nodes, {} edges)",
                            path.display(),
                            def.nodes.len(),
                            def.edges.len()
                        );
                    }
                    None => println!("{json}"),
                }
                return Ok(());
            }
            if let Some(side) = trinity {
                let def = boardgen::generate_trinity(side)
                    .ok_or_else(|| format!("unsupported trinity side {side}; use 4..=26"))?;
                validate_board(&def)
                    .map_err(|e| format!("generated board failed validation: {e}"))?;
                let json = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
                match output {
                    Some(path) => {
                        std::fs::write(&path, json + "\n")
                            .map_err(|e| format!("{}: {e}", path.display()))?;
                        eprintln!(
                            "wrote {} ({} nodes, {} edges)",
                            path.display(),
                            def.nodes.len(),
                            def.edges.len()
                        );
                    }
                    None => println!("{json}"),
                }
                return Ok(());
            }
            let size = size.ok_or("provide --size (hex) or --trinity (triangle)")?;
            let spec = HexBoardSpec {
                radius: match size {
                    19 => 2,
                    37 => 3,
                    61 => 4,
                    91 => 5,
                    127 => 6,
                    other => {
                        return Err(format!(
                            "unsupported size {other}; use 19, 37, 61, 91, or 127"
                        ))
                    }
                },
                portals: match portals.as_str() {
                    "inner6-outer6" => PortalSpec::Inner6Outer6,
                    "inner6" => PortalSpec::Explicit(
                        boardgen::HEX_DIRS.iter().map(|d| [d[0], d[1]]).collect(),
                    ),
                    other => return Err(format!("unknown portal spec {other}")),
                },
            };
            let id = if portals == "inner6-outer6" {
                format!("hex{size}-v1") // standard portal layout keeps the short id
            } else {
                format!("hex{size}-{portals}-v1")
            };
            let def = boardgen::generate(&spec, &id, 1);
            validate_board(&def).map_err(|e| format!("generated board failed validation: {e}"))?;
            let json = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
            match output {
                Some(path) => {
                    std::fs::write(&path, json + "\n")
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    eprintln!(
                        "wrote {} ({} nodes, {} edges)",
                        path.display(),
                        def.nodes.len(),
                        def.edges.len()
                    );
                }
                None => println!("{json}"),
            }
            Ok(())
        }
        Command::Validate { files } => {
            if files.is_empty() {
                return Err("no board files given".to_string());
            }
            let mut failed = false;
            for path in &files {
                match load_board(path)
                    .and_then(|def| validate_board(&def).map_err(|e| e.to_string()).map(|_| def))
                {
                    Ok(def) => println!(
                        "OK   {} ({}: {} nodes, {} edges, {} origins)",
                        path.display(),
                        def.id,
                        def.nodes.len(),
                        def.edges.len(),
                        def.origins.len()
                    ),
                    Err(e) => {
                        failed = true;
                        println!("FAIL {}: {e}", path.display());
                    }
                }
            }
            if failed {
                Err("one or more boards failed validation".to_string())
            } else {
                Ok(())
            }
        }
        Command::Play {
            board,
            pie,
            ruleset,
        } => {
            // Generated id first, file path second.
            let def = match boardgen::resolve(&board) {
                Some(def) => def,
                None => load_board(&PathBuf::from(&board))?,
            };
            let graph = validate_board(&def).map_err(|e| e.to_string())?;
            play::run_interactive(graph, pie, &ruleset)
        }
        Command::Replay {
            record,
            board,
            step,
        } => {
            let def = load_board(&board)?;
            let graph = validate_board(&def).map_err(|e| e.to_string())?;
            let text = std::fs::read_to_string(&record)
                .map_err(|e| format!("{}: {e}", record.display()))?;
            let rec: GameRecord =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", record.display()))?;
            if step {
                let mut game = Game::new(
                    BoardGraph::new(graph.definition().clone()).unwrap(),
                    rec.config.clone(),
                )
                .map_err(|e| e.to_string())?;
                for (i, mv) in rec.moves.iter().enumerate() {
                    let mover = game.to_move();
                    game.play(*mv).map_err(|e| format!("move {i}: {e}"))?;
                    println!("--- ply {} — {} plays {:?} ---", i + 1, mover.name(), mv);
                    println!("{}", ascii::render(&game));
                }
            }
            let game = Game::replay(graph, rec.config, &rec.moves).map_err(|e| e.to_string())?;
            println!("{}", ascii::render(&game));
            match game.result() {
                Some(result) => println!("result: {result:?}"),
                None => println!("game in progress; {} to move", game.to_move().name()),
            }
            // Server-adjudicated endings (timeout, off-turn resignation)
            // are deliberately NOT in the move log — the engine cannot
            // produce them — so a record result the replay lacks is only a
            // contradiction when the replayed game reached a DIFFERENT
            // result, not when it reached none.
            match (&rec.result, game.result()) {
                (Some(recorded), Some(replayed)) if *recorded != replayed => {
                    return Err(format!(
                        "record result {recorded:?} contradicts replayed result {replayed:?}"
                    ));
                }
                (Some(recorded), None) => {
                    println!("server-adjudicated ending: {recorded:?} (not derivable from moves)");
                }
                _ => {}
            }
            Ok(())
        }
    }
}
