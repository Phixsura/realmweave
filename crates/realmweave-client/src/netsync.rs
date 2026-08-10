//! Split from main.rs in the world-class refactor; systems only —
//! shared resources/types stay in `main.rs` (crate root).

use bevy::prelude::*;

#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use realmweave_core::Move;

pub(crate) fn net_pump(
    mut commands: Commands,
    mut session: ResMut<GameSession>,
    mut ui: ResMut<UiState>,
    net: Res<Net>,
    server: Option<Res<ServerAddr>>,
) {
    let Some(handle) = &net.0 else { return };
    let session = &mut session.0;
    while let Ok(event) = handle.rx.try_recv() {
        match event {
            NetEvent::Connected => {}
            NetEvent::Disconnected(reason) => {
                if let Connection::Online { connected, .. } = &mut session.connection {
                    *connected = false;
                }
                ui.status = format!("disconnected: {reason}");
            }
            NetEvent::Message(msg) => match msg {
                ServerMessage::RoomCreated {
                    room_id,
                    token,
                    seat,
                } => {
                    session.control = Control::Seat(seat);
                    session.connection = Connection::Online {
                        room_id,
                        connected: true,
                        opponent_connected: false,
                        token,
                    };
                }
                ServerMessage::Joined {
                    room_id,
                    token,
                    seat,
                } => {
                    session.control = Control::Seat(seat);
                    session.connection = Connection::Online {
                        room_id,
                        connected: true,
                        opponent_connected: true,
                        token,
                    };
                }
                ServerMessage::Snapshot(snap) => {
                    // Authoritative rebuild of the local mirror.
                    let Some(server) = &server else { continue };
                    match net::fetch_board(&server.0, &snap.config.board_id)
                        .and_then(|def| BoardGraph::new(def).map_err(|e| e.to_string()))
                        .and_then(|board| {
                            Game::replay(board, snap.config.clone(), &snap.moves)
                                .map_err(|e| e.to_string())
                        }) {
                        Ok(game) => {
                            session.game = game;
                            session.control = Control::Seat(snap.seat);
                            session.clock = Some(snap.clock);
                            session.server_result = snap.result;
                            if let Connection::Online {
                                opponent_connected, ..
                            } = &mut session.connection
                            {
                                *opponent_connected = snap.opponent_connected;
                            }
                        }
                        Err(e) => ui.status = format!("snapshot error: {e}"),
                    }
                    commands.insert_resource(Active);
                }
                ServerMessage::MoveAccepted(event) => {
                    // Swap events are seat exchanges; a Snapshot follows and
                    // rebuilds — skip local application to avoid divergence.
                    if event.mv != Move::Swap {
                        session.apply_committed(event.mv);
                    }
                    session.clock = Some(event.clock);
                    session.last_error = None;
                }
                ServerMessage::MoveRejected { reason } => {
                    session.last_error = Some(reason);
                }
                ServerMessage::ClockUpdate(clock) => {
                    session.clock = Some(clock);
                }
                ServerMessage::GameEnded { result, clock } => {
                    session.server_result = Some(result);
                    session.clock = Some(clock);
                }
                ServerMessage::OpponentConnection { connected } => {
                    if let Connection::Online {
                        opponent_connected, ..
                    } = &mut session.connection
                    {
                        *opponent_connected = connected;
                    }
                }
                ServerMessage::Error { reason } => {
                    ui.status = reason;
                }
                ServerMessage::Pong => {}
            },
        }
    }
}

/// Automatic reconnect with a simple 3-second backoff while the online
/// connection is down and the game is unfinished.
pub(crate) fn auto_reconnect(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: Local<f32>,
    session: Res<GameSession>,
    ui: Res<UiState>,
) {
    let s = &session.0;
    let Connection::Online {
        connected: false, ..
    } = &s.connection
    else {
        *timer = 0.0;
        return;
    };
    if s.result().is_some() {
        return;
    }
    *timer += time.delta_secs();
    if *timer >= 3.0 {
        *timer = 0.0;
        try_reconnect(&mut commands, s, &ui);
    }
}
