//! End-to-end online tests: two WebSocket clients play through the server.

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use realmweave_core::{boardgen, GameConfig, GameResult, Player, TimeControl, WinReason};
use realmweave_protocol::{decode, encode, ClientMessage, Envelope, ServerMessage};
use realmweave_server::store::Store;
use realmweave_server::{build_app, AppState};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Client {
    ws: Ws,
    seq: u64,
}

impl Client {
    async fn connect(addr: &str) -> Client {
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
            .await
            .expect("connect");
        Client { ws, seq: 0 }
    }

    async fn send(&mut self, msg: ClientMessage) {
        self.seq += 1;
        let frame = encode(&Envelope::new(self.seq, msg));
        self.ws.send(WsMessage::Text(frame.into())).await.unwrap();
    }

    /// Receive the next server message, skipping clock updates.
    async fn recv(&mut self) -> ServerMessage {
        loop {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(5), self.ws.next())
                .await
                .expect("server response within 5s")
                .expect("stream open")
                .expect("frame ok");
            let WsMessage::Text(text) = frame else {
                continue;
            };
            let env: Envelope<ServerMessage> = decode(&text).unwrap();
            if !matches!(env.msg, ServerMessage::ClockUpdate(_)) {
                return env.msg;
            }
        }
    }
}

async fn start_server() -> (String, Arc<AppState>) {
    let mut boards = std::collections::HashMap::new();
    let def = boardgen::generate_standard(19).unwrap();
    boards.insert(def.id.clone(), def);
    let store = Store::open("sqlite::memory:").await.unwrap();
    let state = Arc::new(AppState {
        boards,
        rooms: Mutex::new(Default::default()),
        store,
    });
    let app = build_app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, state)
}

fn quick_config() -> GameConfig {
    GameConfig::new("hex19-v1").with_time_control(TimeControl {
        base_ms: 60_000,
        increment_ms: 0,
    })
}

/// Create a room with client A, join with client B; returns both plus ids.
async fn setup_room(addr: &str) -> (Client, Client, String, String, String) {
    let mut light = Client::connect(addr).await;
    light
        .send(ClientMessage::CreateRoom {
            config: quick_config(),
        })
        .await;
    let (room_id, light_token) = match light.recv().await {
        ServerMessage::RoomCreated {
            room_id,
            token,
            seat,
        } => {
            assert_eq!(seat, Player::Light);
            (room_id, token)
        }
        other => panic!("expected RoomCreated, got {other:?}"),
    };

    let mut dark = Client::connect(addr).await;
    dark.send(ClientMessage::JoinRoom {
        room_id: room_id.clone(),
    })
    .await;
    let dark_token = match dark.recv().await {
        ServerMessage::Joined { token, seat, .. } => {
            assert_eq!(seat, Player::Dark);
            token
        }
        other => panic!("expected Joined, got {other:?}"),
    };

    // Both receive a Snapshot when the game starts.
    for c in [&mut light, &mut dark] {
        match c.recv().await {
            ServerMessage::Snapshot(snap) => assert!(snap.started),
            other => panic!("expected Snapshot, got {other:?}"),
        }
    }
    (light, dark, room_id, light_token, dark_token)
}

#[tokio::test]
async fn two_clients_play_and_server_rejects_illegal_moves() {
    let (addr, _state) = start_server().await;
    let (mut light, mut dark, _room, _lt, _dt) = setup_room(&addr).await;

    // Dark tries to move out of turn → rejected.
    dark.send(ClientMessage::PlayMove { node: 20 }).await;
    assert!(matches!(
        dark.recv().await,
        ServerMessage::MoveRejected { .. }
    ));

    // Light plays; both clients receive MoveAccepted.
    light.send(ClientMessage::PlayMove { node: 20 }).await;
    for c in [&mut light, &mut dark] {
        match c.recv().await {
            ServerMessage::MoveAccepted(ev) => {
                assert_eq!(ev.player, Player::Light);
                assert_eq!(ev.ply, 1);
            }
            other => panic!("expected MoveAccepted, got {other:?}"),
        }
    }

    // Light tries to occupy the same node → rejected (occupied) after Dark
    // hasn't moved yet: also out of turn.
    light.send(ClientMessage::PlayMove { node: 20 }).await;
    assert!(matches!(
        light.recv().await,
        ServerMessage::MoveRejected { .. }
    ));

    // Dark plays the occupied node → rejected by rules.
    dark.send(ClientMessage::PlayMove { node: 20 }).await;
    assert!(matches!(
        dark.recv().await,
        ServerMessage::MoveRejected { .. }
    ));

    // Dark plays a legal node.
    dark.send(ClientMessage::PlayMove { node: 22 }).await;
    for c in [&mut light, &mut dark] {
        assert!(matches!(c.recv().await, ServerMessage::MoveAccepted(_)));
    }
}

#[tokio::test]
async fn resignation_ends_game_and_persists_replayable_record() {
    let (addr, state) = start_server().await;
    let (mut light, mut dark, _room, _lt, _dt) = setup_room(&addr).await;

    light.send(ClientMessage::PlayMove { node: 20 }).await;
    for c in [&mut light, &mut dark] {
        assert!(matches!(c.recv().await, ServerMessage::MoveAccepted(_)));
    }
    dark.send(ClientMessage::Resign).await;
    for c in [&mut light, &mut dark] {
        assert!(matches!(c.recv().await, ServerMessage::MoveAccepted(_)));
        match c.recv().await {
            ServerMessage::GameEnded { result, .. } => {
                assert_eq!(
                    result,
                    GameResult::Win {
                        player: Player::Light,
                        reason: WinReason::Resignation
                    }
                );
            }
            other => panic!("expected GameEnded, got {other:?}"),
        }
    }

    // Record persisted and replayable.
    let game_id = {
        let rooms = state.rooms.lock().await;
        let room = rooms.values().next().unwrap().lock().await;
        room.game_id.clone()
    };
    let record = state.store.load_record(&game_id).await.unwrap().unwrap();
    assert_eq!(record.moves.len(), 2);
    assert!(record.result.is_some());
    let board = realmweave_core::BoardGraph::new(boardgen::generate_standard(19).unwrap()).unwrap();
    let replayed = realmweave_core::Game::replay(board, record.config, &record.moves).unwrap();
    assert_eq!(replayed.result(), record.result);
}

#[tokio::test]
async fn reconnect_restores_seat_with_snapshot() {
    let (addr, _state) = start_server().await;
    let (mut light, mut dark, room_id, _lt, dark_token) = setup_room(&addr).await;

    light.send(ClientMessage::PlayMove { node: 20 }).await;
    for c in [&mut light, &mut dark] {
        assert!(matches!(c.recv().await, ServerMessage::MoveAccepted(_)));
    }

    // Dark drops.
    dark.ws.close(None).await.unwrap();
    match light.recv().await {
        ServerMessage::OpponentConnection { connected } => assert!(!connected),
        other => panic!("expected OpponentConnection, got {other:?}"),
    }

    // Dark reconnects with its token.
    let mut dark2 = Client::connect(&addr).await;
    dark2
        .send(ClientMessage::Reconnect {
            room_id: room_id.clone(),
            token: dark_token,
        })
        .await;
    match dark2.recv().await {
        ServerMessage::Snapshot(snap) => {
            assert_eq!(snap.seat, Player::Dark);
            assert_eq!(snap.moves.len(), 1);
            assert!(snap.started);
        }
        other => panic!("expected Snapshot, got {other:?}"),
    }
    match light.recv().await {
        ServerMessage::OpponentConnection { connected } => assert!(connected),
        other => panic!("expected OpponentConnection, got {other:?}"),
    }

    // Play continues after reconnect.
    dark2.send(ClientMessage::PlayMove { node: 22 }).await;
    assert!(matches!(dark2.recv().await, ServerMessage::MoveAccepted(_)));
    assert!(matches!(light.recv().await, ServerMessage::MoveAccepted(_)));

    // Bad token is rejected.
    let mut intruder = Client::connect(&addr).await;
    intruder
        .send(ClientMessage::Reconnect {
            room_id,
            token: "wrong".to_string(),
        })
        .await;
    assert!(matches!(intruder.recv().await, ServerMessage::Error { .. }));
}

#[tokio::test]
async fn third_client_cannot_join_full_room() {
    let (addr, _state) = start_server().await;
    let (_light, _dark, room_id, _lt, _dt) = setup_room(&addr).await;
    let mut spectator = Client::connect(&addr).await;
    spectator.send(ClientMessage::JoinRoom { room_id }).await;
    assert!(matches!(
        spectator.recv().await,
        ServerMessage::Error { .. }
    ));
}

#[tokio::test]
async fn stale_sequence_numbers_rejected() {
    let (addr, _state) = start_server().await;
    let mut c = Client::connect(&addr).await;
    c.send(ClientMessage::Ping).await;
    assert!(matches!(c.recv().await, ServerMessage::Pong));
    // Re-send with a stale seq (manually).
    let frame = encode(&Envelope::new(1, ClientMessage::Ping));
    c.ws.send(WsMessage::Text(frame.into())).await.unwrap();
    assert!(matches!(c.recv().await, ServerMessage::Error { .. }));
}

#[tokio::test]
async fn clock_timeout_ends_game() {
    let (addr, _state) = start_server().await;
    // 300ms base: Light flags almost immediately.
    let mut light = Client::connect(&addr).await;
    light
        .send(ClientMessage::CreateRoom {
            config: GameConfig::new("hex19-v1").with_time_control(TimeControl {
                base_ms: 300,
                increment_ms: 0,
            }),
        })
        .await;
    let room_id = match light.recv().await {
        ServerMessage::RoomCreated { room_id, .. } => room_id,
        other => panic!("expected RoomCreated, got {other:?}"),
    };
    let mut dark = Client::connect(&addr).await;
    dark.send(ClientMessage::JoinRoom { room_id }).await;
    assert!(matches!(dark.recv().await, ServerMessage::Joined { .. }));
    for c in [&mut light, &mut dark] {
        assert!(matches!(c.recv().await, ServerMessage::Snapshot(_)));
    }
    // Wait for the flag: server clock tick detects it.
    match light.recv().await {
        ServerMessage::GameEnded { result, .. } => {
            assert_eq!(
                result,
                GameResult::Win {
                    player: Player::Dark,
                    reason: WinReason::Timeout
                }
            );
        }
        other => panic!("expected GameEnded by timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn pie_rule_swap_exchanges_seats() {
    let (addr, _state) = start_server().await;
    let mut light = Client::connect(&addr).await;
    light
        .send(ClientMessage::CreateRoom {
            config: GameConfig::new("hex19-v1").with_pie_rule(true),
        })
        .await;
    let room_id = match light.recv().await {
        ServerMessage::RoomCreated { room_id, .. } => room_id,
        other => panic!("unexpected {other:?}"),
    };
    let mut dark = Client::connect(&addr).await;
    dark.send(ClientMessage::JoinRoom { room_id }).await;
    assert!(matches!(dark.recv().await, ServerMessage::Joined { .. }));
    for c in [&mut light, &mut dark] {
        assert!(matches!(c.recv().await, ServerMessage::Snapshot(_)));
    }

    light.send(ClientMessage::PlayMove { node: 20 }).await;
    for c in [&mut light, &mut dark] {
        assert!(matches!(c.recv().await, ServerMessage::MoveAccepted(_)));
    }

    // Dark swaps: seats exchange, so the original Dark connection now plays
    // Light and vice versa.
    dark.send(ClientMessage::SwapSides).await;
    for c in [&mut light, &mut dark] {
        assert!(matches!(c.recv().await, ServerMessage::MoveAccepted(_)));
    }
    match dark.recv().await {
        ServerMessage::Snapshot(snap) => assert_eq!(snap.seat, Player::Light),
        other => panic!("expected Snapshot, got {other:?}"),
    }
    match light.recv().await {
        ServerMessage::Snapshot(snap) => assert_eq!(snap.seat, Player::Dark),
        other => panic!("expected Snapshot, got {other:?}"),
    }

    // After the swap it is still (color) Dark's turn to place — that is the
    // original creator's connection now.
    light.send(ClientMessage::PlayMove { node: 22 }).await;
    assert!(matches!(light.recv().await, ServerMessage::MoveAccepted(_)));
}
