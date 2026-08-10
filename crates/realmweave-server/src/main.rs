//! Realmweave authoritative online server binary.

#![allow(clippy::expect_used)] // binary entrypoint: fail fast at startup
use std::sync::Arc;

use clap::Parser;
use realmweave_server::store::Store;
use realmweave_server::{build_app, load_boards, AppState};
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(name = "realmweave-server")]
struct Args {
    /// Listen address.
    #[arg(long, default_value = "127.0.0.1:8420")]
    listen: String,
    /// Directory containing board definition JSON files.
    #[arg(long, default_value = "boards")]
    boards: String,
    /// SQLite database path.
    #[arg(long, default_value = "sqlite://realmweave.db")]
    db: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let boards = load_boards(&args.boards);
    tracing::info!(
        "loaded {} boards: {:?}",
        boards.len(),
        boards.keys().collect::<Vec<_>>()
    );

    let store = Store::open(&args.db).await.expect("open database");
    let state = Arc::new(AppState {
        boards,
        rooms: Mutex::new(Default::default()),
        store,
    });

    realmweave_server::spawn_room_reaper(state.clone());
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind(&args.listen)
        .await
        .expect("bind listen address");
    tracing::info!("listening on {}", args.listen);
    axum::serve(listener, app).await.expect("server run");
}
