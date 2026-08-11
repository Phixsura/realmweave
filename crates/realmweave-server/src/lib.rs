//! Realmweave authoritative online server library.
//!
//! - Server-owned game state and clocks; clients send intent only.
//! - One WebSocket per seated player; canonical events carry seq numbers.
//! - SQLite persistence of games + ordered event logs (replayable).

pub mod room;
pub mod store;

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use realmweave_core::{validate_board, BoardDefinition, BoardGraph, Game, Move, Player};
use realmweave_protocol::{decode, encode, ClientMessage, Envelope, ServerMessage};
use room::Room;
use store::Store;
use tokio::sync::{mpsc, Mutex};

/// Process-wide server state shared across connections.
pub struct AppState {
    /// Validated boards, keyed by board id.
    pub boards: HashMap<String, BoardDefinition>,
    /// Live rooms, keyed by room code.
    pub rooms: Mutex<HashMap<String, Arc<Mutex<Room>>>>,
    /// Event persistence.
    pub store: Store,
}

/// Shared handle to [`AppState`].
pub type Shared = Arc<AppState>;

/// Load and validate all board JSON files in a directory.
///
/// Startup-time only: a malformed shipped board is a deployment error, so
/// failing fast (with the offending path in the panic message) is correct.
#[allow(clippy::expect_used)]
pub fn load_boards(dir: &str) -> HashMap<String, BoardDefinition> {
    let mut boards = HashMap::new();
    for entry in std::fs::read_dir(dir).expect("boards directory") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let text = std::fs::read_to_string(&path).expect("readable board file");
            let def: BoardDefinition = serde_json::from_str(&text).expect("valid board JSON");
            validate_board(&def).expect("board passes validation");
            boards.insert(def.id.clone(), def);
        }
    }
    boards
}

/// Periodically remove rooms that are finished (or never started) and have
/// both seats disconnected for longer than the grace period. Without this a
/// long-running server retains every room ever created.
pub fn spawn_room_reaper(state: Shared) {
    const SWEEP_EVERY: std::time::Duration = std::time::Duration::from_secs(60);
    const GRACE: std::time::Duration = std::time::Duration::from_secs(15 * 60);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_EVERY).await;
            let mut rooms = state.rooms.lock().await;
            let mut doomed = Vec::new();
            for (code, room) in rooms.iter() {
                let mut room = room.lock().await;
                // Flag-fall adjudication happens HERE too: per-connection
                // clock tickers die with their sockets, so a live game with
                // both players gone would otherwise never time out — and a
                // losing player could freeze the game by disconnecting.
                if room.started && !room.finished {
                    if let Some(flagged) = room.flagged() {
                        let result = room.timeout(flagged);
                        let clock = room.clock();
                        room.broadcast(ServerMessage::GameEnded { result, clock });
                        let _ = state.store.finish_game(&room.game_id, &result).await;
                        tracing::info!(room = %code, "adjudicated timeout in reaper");
                    }
                }
                let idle = room.last_activity.elapsed() > GRACE;
                // Untimed live games have no flag-fall, so with both seats
                // gone they would otherwise be unreapable forever — a
                // trivially mintable permanent memory leak. Abandonment
                // (both disconnected past the grace period) reaps them too;
                // reconnect tokens die with the room, which is the same
                // contract as a finished room.
                let reapable = room.finished || !room.started || room.time_control.is_none();
                if reapable && room.fully_disconnected() && idle {
                    doomed.push(code.clone());
                }
            }
            for code in doomed {
                rooms.remove(&code);
                tracing::info!(room = %code, "reaped idle room");
            }
        }
    });
}

/// Build the axum application router.
pub fn build_app(state: Shared) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/boards/{id}", get(get_board))
        .route("/api/games/{id}/record", get(get_record))
        .route("/ws", get(ws_upgrade))
        .with_state(state)
}

/// Liveness + basic operational stats (rooms open, boards loaded).
async fn healthz(State(state): State<Shared>) -> impl IntoResponse {
    let rooms = state.rooms.lock().await.len();
    (
        axum::http::StatusCode::OK,
        format!(
            "{{\"status\":\"ok\",\"rooms\":{rooms},\"boards\":{},\"version\":\"{}\"}}",
            state.boards.len(),
            env!("CARGO_PKG_VERSION"),
        ),
    )
}

async fn get_board(State(state): State<Shared>, Path(id): Path<String>) -> impl IntoResponse {
    match state.boards.get(&id) {
        Some(def) => (
            axum::http::StatusCode::OK,
            serde_json::to_string(def).unwrap_or_default(),
        ),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            "unknown board".to_string(),
        ),
    }
}

/// Export a completed (or live) game as a replayable GameRecord.
async fn get_record(State(state): State<Shared>, Path(id): Path<String>) -> impl IntoResponse {
    match state.store.load_record(&id).await {
        Ok(Some(record)) => (
            axum::http::StatusCode::OK,
            serde_json::to_string_pretty(&record).unwrap_or_default(),
        ),
        Ok(None) => (
            axum::http::StatusCode::NOT_FOUND,
            "unknown game".to_string(),
        ),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn ws_upgrade(State(state): State<Shared>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket))
}

fn room_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Per-connection state after seating. The seat color is always derived
/// from the reconnect token so pie-rule seat swaps can never go stale.
struct Session {
    room: Arc<Mutex<Room>>,
    token: String,
}

async fn handle_socket(state: Shared, socket: WebSocket) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Outbound pump: everything the room (or this handler) sends goes
    // through `tx` and is serialized here with the canonical event seq of 0
    // for connection-level messages (room events carry their own seq inside).
    let mut out_seq: u64 = 0;
    let writer = tokio::spawn(async move {
        // Slow-reader guard: a client that stops reading its socket stalls
        // `ws_tx.send` here while the room keeps pushing ClockUpdates into
        // the unbounded queue — unbounded heap growth per hostile socket.
        // A healthy client is never thousands of messages behind; drop it.
        const MAX_QUEUED: usize = 4096;
        while let Some(msg) = rx.recv().await {
            if rx.len() > MAX_QUEUED {
                tracing::warn!("outbound queue overflow; dropping slow connection");
                break;
            }
            out_seq += 1;
            let frame = encode(&Envelope::new(out_seq, msg));
            if ws_tx.send(Message::Text(frame.into())).await.is_err() {
                break;
            }
        }
    });

    let mut session: Option<Session> = None;
    let mut last_client_seq: u64 = 0;
    // Rate limiting: simple token bucket, 20 commands / 5 seconds.
    let mut bucket: u32 = 20;
    let mut bucket_refill = tokio::time::interval(std::time::Duration::from_millis(250));
    bucket_refill.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Clock ticker: pushes updates & detects flag fall while a game is live.
    let mut clock_tick = tokio::time::interval(std::time::Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = bucket_refill.tick() => {
                bucket = (bucket + 1).min(20);
            }
            _ = clock_tick.tick() => {
                if let Some(Session { room, .. }) = &session {
                    let mut room = room.lock().await;
                    if room.started && !room.finished {
                        if let Some(flagged) = room.flagged() {
                            let result = room.timeout(flagged);
                            let clock = room.clock();
                            room.broadcast(ServerMessage::GameEnded { result, clock });
                            let _ = state.store.finish_game(&room.game_id, &result).await;
                        } else if room.time_control.is_some() {
                            room.broadcast(ServerMessage::ClockUpdate(room.clock()));
                        }
                    }
                }
            }
            frame = ws_rx.next() => {
                let Some(Ok(frame)) = frame else { break };
                let Message::Text(text) = frame else { continue };
                if bucket == 0 {
                    let _ = tx.send(ServerMessage::Error { reason: "rate limited".into() });
                    continue;
                }
                bucket -= 1;
                let envelope: Envelope<ClientMessage> = match decode(&text) {
                    Ok(env) => env,
                    Err(e) => {
                        let _ = tx.send(ServerMessage::Error { reason: e.to_string() });
                        continue;
                    }
                };
                // Reject duplicate/stale command sequence numbers.
                if envelope.seq <= last_client_seq {
                    let _ = tx.send(ServerMessage::Error {
                        reason: format!("stale seq {} (last {})", envelope.seq, last_client_seq),
                    });
                    continue;
                }
                last_client_seq = envelope.seq;
                handle_message(&state, &tx, &mut session, envelope.msg).await;
            }
        }
    }

    disconnect_seat(session, &tx).await;
    writer.abort();
}

/// Mark the seat as away and notify the opponent. Compares channel
/// IDENTITY, not just the token: a half-dead socket's teardown can fire
/// after the same player already reconnected on a new socket, and
/// clearing the fresh channel would silently disconnect the live
/// connection (every broadcast dropped until the next reconnect).
async fn disconnect_seat(session: Option<Session>, tx: &mpsc::UnboundedSender<ServerMessage>) {
    let Some(Session { room, token }) = session else {
        return;
    };
    let mut room = room.lock().await;
    room.last_activity = std::time::Instant::now();
    let Some(seat) = room.seat_of_token(&token) else {
        return;
    };
    let is_this_connection = room
        .seat_mut(seat)
        .and_then(|s| s.tx.as_ref())
        .is_some_and(|t| t.same_channel(tx));
    if is_this_connection {
        if let Some(s) = room.seat_mut(seat) {
            s.tx = None;
        }
        room.send_to(
            seat.opponent(),
            ServerMessage::OpponentConnection { connected: false },
        );
    }
}

async fn handle_message(
    state: &Shared,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    session: &mut Option<Session>,
    msg: ClientMessage,
) {
    match msg {
        ClientMessage::Ping => {
            let _ = tx.send(ServerMessage::Pong);
        }
        ClientMessage::CreateRoom { config } => {
            tracing::info!(ruleset = %config.ruleset_id, board = %config.board_id, "room create requested");
            if session.is_some() {
                let _ = tx.send(ServerMessage::Error {
                    reason: "already seated".into(),
                });
                return;
            }
            // Global room cap: the per-connection rate limit does not stop
            // an attacker opening N sockets and creating one room each —
            // every room is heap state AND a permanent SQLite games row.
            // Legitimate concurrent-room counts are tiny; the cap is a DoS
            // backstop, not a product limit.
            const MAX_LIVE_ROOMS: usize = 1024;
            if state.rooms.lock().await.len() >= MAX_LIVE_ROOMS {
                let _ = tx.send(ServerMessage::Error {
                    reason: "server is at capacity, try again later".into(),
                });
                return;
            }
            let Some(def) = state.boards.get(&config.board_id) else {
                let _ = tx.send(ServerMessage::Error {
                    reason: format!("unknown board {}", config.board_id),
                });
                return;
            };
            let graph = match BoardGraph::new(def.clone()) {
                Ok(g) => g,
                Err(e) => {
                    let _ = tx.send(ServerMessage::Error {
                        reason: e.to_string(),
                    });
                    return;
                }
            };
            let game = match Game::new(graph, config.clone()) {
                Ok(g) => g,
                Err(e) => {
                    let _ = tx.send(ServerMessage::Error {
                        reason: e.to_string(),
                    });
                    return;
                }
            };
            let game_id = uuid::Uuid::new_v4().to_string();
            let token = uuid::Uuid::new_v4().to_string();
            // Uniqueness check and insert under ONE lock acquisition:
            // releasing between them lets two concurrent creates pick the
            // same code, and HashMap::insert would silently replace the
            // first room (orphaning it — joinable by nobody).
            let (room_id, room) = {
                let mut rooms = state.rooms.lock().await;
                let mut code = room_code();
                while rooms.contains_key(&code) {
                    code = room_code();
                }
                let mut room = Room::new(code.clone(), game_id.clone(), game, token.clone());
                room.light.tx = Some(unbounded_to(tx));
                let room = Arc::new(Mutex::new(room));
                rooms.insert(code.clone(), room.clone());
                (code, room)
            };
            if let Err(e) = state.store.create_game(&game_id, &config).await {
                tracing::error!("persist create_game: {e}");
            }
            *session = Some(Session {
                room,
                token: token.clone(),
            });
            let _ = tx.send(ServerMessage::RoomCreated {
                room_id,
                token,
                seat: Player::Light,
            });
        }
        ClientMessage::JoinRoom { room_id } => {
            tracing::info!(room = %room_id, "join requested");
            if session.is_some() {
                let _ = tx.send(ServerMessage::Error {
                    reason: "already seated".into(),
                });
                return;
            }
            let room_arc = state
                .rooms
                .lock()
                .await
                .get(&room_id.to_uppercase())
                .cloned();
            let Some(room_arc) = room_arc else {
                let _ = tx.send(ServerMessage::Error {
                    reason: "no such room".into(),
                });
                return;
            };
            let mut room = room_arc.lock().await;
            if room.dark.is_some() {
                // Spectators are disabled in the MVP.
                let _ = tx.send(ServerMessage::Error {
                    reason: "room is full".into(),
                });
                return;
            }
            let token = uuid::Uuid::new_v4().to_string();
            room.last_activity = std::time::Instant::now();
            room.dark = Some(room::Seat {
                token: token.clone(),
                tx: Some(unbounded_to(tx)),
            });
            let _ = tx.send(ServerMessage::Joined {
                room_id: room.id.clone(),
                token: token.clone(),
                seat: Player::Dark,
            });
            if room.start_if_ready() {
                for seat in [Player::Light, Player::Dark] {
                    let snapshot = room.snapshot_for(seat);
                    room.send_to(seat, ServerMessage::Snapshot(snapshot));
                }
            }
            drop(room);
            *session = Some(Session {
                room: room_arc,
                token,
            });
        }
        ClientMessage::Reconnect { room_id, token } => {
            tracing::info!(room = %room_id, "reconnect requested");
            if session.is_some() {
                let _ = tx.send(ServerMessage::Error {
                    reason: "already seated".into(),
                });
                return;
            }
            let room_arc = state
                .rooms
                .lock()
                .await
                .get(&room_id.to_uppercase())
                .cloned();
            let Some(room_arc) = room_arc else {
                let _ = tx.send(ServerMessage::Error {
                    reason: "no such room".into(),
                });
                return;
            };
            let mut room = room_arc.lock().await;
            let Some(seat) = room.seat_of_token(&token) else {
                let _ = tx.send(ServerMessage::Error {
                    reason: "invalid reconnect token".into(),
                });
                return;
            };
            room.last_activity = std::time::Instant::now();
            if let Some(s) = room.seat_mut(seat) {
                s.tx = Some(unbounded_to(tx));
            }
            let snapshot = room.snapshot_for(seat);
            let _ = tx.send(ServerMessage::Snapshot(snapshot));
            room.send_to(
                seat.opponent(),
                ServerMessage::OpponentConnection { connected: true },
            );
            drop(room);
            *session = Some(Session {
                room: room_arc,
                token,
            });
        }
        ClientMessage::PlayMove { node } => {
            play(state, tx, session, Move::Place(node)).await;
        }
        ClientMessage::SeverStone { node } => {
            play(state, tx, session, Move::Sever(node)).await;
        }
        ClientMessage::CutEdge { edge } => {
            play(state, tx, session, Move::CutEdge(edge)).await;
        }
        ClientMessage::Pass => {
            play(state, tx, session, Move::Pass).await;
        }
        ClientMessage::SwapSides => {
            // Pie-rule swap: colors are engine-stable; the server swaps the
            // *people* by exchanging seat tokens/channels after the move.
            let Some(Session { room, token }) = session else {
                let _ = tx.send(ServerMessage::Error {
                    reason: "not seated".into(),
                });
                return;
            };
            let mut room_guard = room.lock().await;
            let Some(seat) = room_guard.seat_of_token(token) else {
                let _ = tx.send(ServerMessage::Error {
                    reason: "not seated".into(),
                });
                return;
            };
            match room_guard.play(seat, Move::Swap) {
                Ok(event) => {
                    // Exchange seats: the swapper (Dark seat holder) becomes
                    // Light and vice versa.
                    {
                        let room::Room { light, dark, .. } = &mut *room_guard;
                        if let Some(dark) = dark.as_mut() {
                            std::mem::swap(&mut light.token, &mut dark.token);
                            std::mem::swap(&mut light.tx, &mut dark.tx);
                        }
                    }
                    let game_id = room_guard.game_id.clone();
                    let clock_json = serde_json::to_string(&event.clock).unwrap_or_default();
                    if let Err(e) = state
                        .store
                        .append_event(
                            &game_id,
                            event.seq,
                            event.ply,
                            event.player,
                            &event.mv,
                            &clock_json,
                        )
                        .await
                    {
                        tracing::error!("persist event: {e}");
                    }
                    room_guard.last_activity = std::time::Instant::now();
                    room_guard.broadcast(ServerMessage::MoveAccepted(event));
                    // Refresh both seats' view of which color they play.
                    for s in [Player::Light, Player::Dark] {
                        let snapshot = room_guard.snapshot_for(s);
                        room_guard.send_to(s, ServerMessage::Snapshot(snapshot));
                    }
                }
                Err(reason) => {
                    let _ = tx.send(ServerMessage::MoveRejected { reason });
                }
            }
        }
        ClientMessage::Resign => {
            play(state, tx, session, Move::Resign).await;
        }
    }
}

async fn play(
    state: &Shared,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    session: &mut Option<Session>,
    mv: Move,
) {
    let Some(Session { room, token }) = session else {
        let _ = tx.send(ServerMessage::Error {
            reason: "not seated".into(),
        });
        return;
    };
    let mut room = room.lock().await;
    let Some(seat) = room.seat_of_token(token) else {
        let _ = tx.send(ServerMessage::Error {
            reason: "not seated".into(),
        });
        return;
    };
    match room.play(seat, mv) {
        Ok(event) => {
            // Off-turn resignations are server adjudications, not engine
            // moves: keep them OUT of the replayable move log (the games
            // row's result carries the outcome). Persisting them would
            // make the exported record unreplayable — the engine would
            // attribute the resignation to the wrong player.
            let engine_move = !(event.mv == Move::Resign
                && room.game.result().is_none()
                && room.result_override.is_some());
            if engine_move {
                let clock_json = serde_json::to_string(&event.clock).unwrap_or_default();
                if let Err(e) = state
                    .store
                    .append_event(
                        &room.game_id,
                        event.seq,
                        event.ply,
                        event.player,
                        &event.mv,
                        &clock_json,
                    )
                    .await
                {
                    tracing::error!("persist event: {e}");
                }
            }
            let result = room.result();
            room.last_activity = std::time::Instant::now();
            // Server adjudications are NOT moves: broadcasting a fake
            // MoveAccepted{Resign} tempts clients to feed it through the
            // engine, which attributes the resignation to the WRONG player
            // (the winner). GameEnded below is the authoritative message;
            // it is all an adjudication needs.
            if engine_move {
                room.broadcast(ServerMessage::MoveAccepted(event));
            }
            if let Some(result) = result {
                let clock = room.clock();
                room.broadcast(ServerMessage::GameEnded { result, clock });
                if let Err(e) = state.store.finish_game(&room.game_id, &result).await {
                    tracing::error!("persist finish: {e}");
                }
            }
        }
        Err(reason) => {
            let _ = tx.send(ServerMessage::MoveRejected { reason });
        }
    }
}

/// Clone an unbounded sender as a room seat channel.
fn unbounded_to(tx: &mpsc::UnboundedSender<ServerMessage>) -> room::Tx {
    tx.clone()
}
