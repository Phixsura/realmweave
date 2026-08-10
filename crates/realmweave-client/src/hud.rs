//! Split from main.rs in the world-class refactor; systems only —
//! shared resources/types stay in `main.rs` (crate root).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use realmweave_core::Move;

#[allow(clippy::too_many_arguments)]
pub(crate) fn game_hud(
    mut commands: Commands,
    mut egui_ctx: EguiContexts,
    session: Res<GameSession>,
    mut view: ResMut<ViewSettings>,
    mut events: EventWriter<IntentEvent>,
    mut ui_state: ResMut<UiState>,
    net: Res<Net>,
    mut replay: Option<ResMut<Replay>>,
    nodes: Query<Entity, With<NodeMarker>>,
) {
    let ctx = egui_ctx.ctx_mut();
    let s = &session.0;
    egui::TopBottomPanel::top("hud").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Realmweave").strong());
            ui.separator();
            ui.label(format!("board {}", s.game.board().definition().id));
            ui.separator();

            match s.result() {
                Some(GameResult::Win { player, reason }) => {
                    let reason = match reason {
                        WinReason::RealmWeave => "realm weave",
                        WinReason::Strangle => "strangle",
                        WinReason::Territory => "territory",
                        WinReason::Resignation => "resignation",
                        WinReason::Timeout => "timeout",
                    };
                    ui.colored_label(
                        egui::Color32::GOLD,
                        format!("{} wins by {reason}", player.name()),
                    );
                }
                Some(GameResult::Draw) => {
                    ui.colored_label(egui::Color32::GOLD, "draw");
                }
                None => {
                    ui.label(format!("{} to move", s.game.to_move().name()));
                    if let Some(pending) = s.game.state().pending_weave {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!("{} has a provisional weave!", pending.name()),
                        );
                    }
                }
            }
            ui.separator();

            if let Some(clock) = &s.clock {
                ui.label(format!(
                    "Light {}  Dark {}",
                    fmt_clock(clock.light_ms),
                    fmt_clock(clock.dark_ms)
                ));
                ui.separator();
            }

            let view_label = match view.mode {
                ViewMode::Stacked3D => "switch to 2D analysis",
                ViewMode::Analysis2D => "switch to 3D realms",
            };
            if ui.button(view_label).clicked() {
                view.mode = view.mode.toggle();
            }
            ui.checkbox(&mut view.show_legal, "legal moves");
            ui.checkbox(&mut view.show_components, "weave highlight");
            if matches!(
                s.game.config().ruleset_id.as_str(),
                realmweave_core::WEAVE_SEVER_V2 | realmweave_core::WEAVE_LAYERS_V3
            ) {
                let label = if view.cut_mode {
                    "✂ 剪线模式 (点两个端点)"
                } else {
                    "落子模式"
                };
                if ui
                    .selectable_label(view.cut_mode, label)
                    .on_hover_text("Tab 切换：剪线模式下点击一条边的两个端点")
                    .clicked()
                {
                    view.cut_mode = !view.cut_mode;
                    view.cut_anchor = None;
                }
            }

            let observing = matches!(s.control, Control::Observer);
            if !observing {
                if s.swap_available() && ui.button("swap sides (pie)").clicked() {
                    events.send(IntentEvent(PlayerIntent::Swap));
                }
                if s.result().is_none()
                    && s.game.legal_moves().contains(&Move::Pass)
                    && ui.button("pass").clicked()
                {
                    events.send(IntentEvent(PlayerIntent::Pass));
                }
                if s.result().is_none() && ui.button("resign").clicked() {
                    events.send(IntentEvent(PlayerIntent::Resign));
                }
            }
            let charges = s.game.state().sever_charges;
            if charges != [0, 0] {
                ui.label(format!("severs L:{} D:{}", charges[0], charges[1]));
            }
            if s.game.config().ruleset_id == realmweave_core::TRINITY_Y_V4 {
                let ly = s.game.state().layers;
                ui.label(
                    egui::RichText::new(format!(
                        "⚖ 界域 白 {} | 黑 {} （先取两界胜）",
                        ly[0], ly[1]
                    ))
                    .strong()
                    .size(16.0),
                );
            }
            if s.game.config().ruleset_id == realmweave_core::WEAVE_LAYERS_V3 {
                let ly = s.game.state().layers;
                ui.label(
                    egui::RichText::new(format!(
                        "🕸 层数 白 {}/{} | 黑 {}/{}",
                        ly[0],
                        realmweave_core::rules::LAYERS_TO_WIN,
                        ly[1],
                        realmweave_core::rules::LAYERS_TO_WIN
                    ))
                    .strong()
                    .size(16.0),
                );
            }
            let is_scissor_rules = matches!(
                s.game.config().ruleset_id.as_str(),
                realmweave_core::WEAVE_SEVER_V2 | realmweave_core::WEAVE_LAYERS_V3
            );
            if is_scissor_rules {
                let sc = s.game.state().scissors;
                ui.label(egui::RichText::new(format!("✂ 白 {} | 黑 {}", sc[0], sc[1])).strong());
                // Lifelines: potential origin groups (1 = healthy).
                let lg = realmweave_core::rules::potential_origin_groups(
                    s.game.board(),
                    s.game.state(),
                    Player::Light,
                );
                let dg = realmweave_core::rules::potential_origin_groups(
                    s.game.board(),
                    s.game.state(),
                    Player::Dark,
                );
                let text = format!("生命线 白:{} 黑:{}", lifeline(lg), lifeline(dg));
                ui.label(text);
                // Strangle danger: >1 potential group means part of your
                // origins can never reconnect — permanent damage.
                if s.result().is_none() {
                    if lg > 1 {
                        ui.colored_label(egui::Color32::LIGHT_RED, "⚠ 白方起源已被割裂！");
                    }
                    if dg > 1 {
                        ui.colored_label(egui::Color32::LIGHT_RED, "⚠ 黑方起源已被割裂！");
                    }
                }
            }
            if let Some(text) = s.last_move_text() {
                ui.separator();
                ui.label(egui::RichText::new(text).italics());
            }
            if ui.button("leave").clicked() {
                commands.remove_resource::<Tutorial>();
                commands.remove_resource::<Duel>();
                commands.remove_resource::<Active>();
                commands.remove_resource::<GameSession>();
                commands.remove_resource::<Net>();
                commands.remove_resource::<BoardSpawned>();
                commands.remove_resource::<Palette>();
                commands.remove_resource::<Replay>();
                for entity in &nodes {
                    commands.entity(entity).despawn();
                }
                ui_state.status.clear();
            }
        });
        // Second row: connection status / errors.
        ui.horizontal(|ui| {
            match &s.connection {
                Connection::Local => match s.control {
                    Control::VsBot(human) => {
                        ui.label(format!("人机对战 — 你执{}", human.name()));
                        if s.result().is_none() && s.game.to_move() != human {
                            ui.colored_label(egui::Color32::YELLOW, "AI 思考中…");
                        }
                    }
                    _ => {
                        ui.label("local hot-seat");
                    }
                },
                Connection::Online {
                    room_id,
                    connected,
                    opponent_connected,
                    ..
                } => {
                    ui.label(format!("room {room_id}"));
                    if let Control::Seat(p) = s.control {
                        ui.label(format!("you play {}", p.name()));
                    }
                    if !connected {
                        ui.colored_label(egui::Color32::LIGHT_RED, "connection lost");
                        if ui.button("reconnect").clicked() {
                            try_reconnect(&mut commands, s, &ui_state);
                        }
                    } else if !opponent_connected {
                        ui.colored_label(egui::Color32::YELLOW, "waiting for opponent…");
                    }
                }
            }
            if let Some(err) = &s.last_error {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }
            let caps = s.game.state().captures;
            if caps != [0, 0] {
                ui.label(format!("提子 白{}:黑{}", caps[0], caps[1]));
            }
            if let Some(hovered) = view.hovered {
                ui.separator();
                ui.label(node_tooltip(s, hovered));
            }
            let _ = &net;
        });
        // Replay transport bar.
        if let Some(replay) = replay.as_mut() {
            // Commentary for the current move (demo/teaching mode).
            if let Some(note) = replay.0.current_annotation() {
                let heading = format!("第 {} 手 — {}", note.ply, note.player);
                let body = note.text.clone();
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(heading)
                            .strong()
                            .color(egui::Color32::GOLD),
                    );
                    ui.label(body);
                });
            }
            ui.horizontal(|ui| {
                ui.label("replay");
                // Auto-play toggle + countdown.
                if replay.0.auto_seconds > 0.0 {
                    if ui.button("⏸ pause").clicked() {
                        replay.0.auto_seconds = 0.0;
                    } else {
                        ui.label(format!(
                            "auto: next in {:.0}s",
                            replay.0.auto_timer.max(0.0)
                        ));
                    }
                } else if ui.button("▶ auto 30s").clicked() {
                    replay.0.auto_seconds = 30.0;
                    replay.0.auto_timer = 0.5;
                }
                let len = replay.0.len();
                let mut cursor = replay.0.cursor;
                if ui.button("|<").clicked() {
                    cursor = 0;
                }
                if ui.button("<").clicked() {
                    cursor = cursor.saturating_sub(1);
                }
                if ui.button(">").clicked() {
                    cursor = (cursor + 1).min(len);
                }
                if ui.button(">|").clicked() {
                    cursor = len;
                }
                let mut cursor_f = cursor as f32;
                ui.add(egui::Slider::new(&mut cursor_f, 0.0..=len as f32).integer());
                cursor = cursor_f as usize;
                if cursor != replay.0.cursor {
                    replay.0.seek(cursor);
                }
                ui.label(format!("{cursor}/{len}"));
                if let Some(result) = &replay.0.record.result {
                    ui.label(format!("final: {result:?}"));
                }
            });
        }
    });
}

/// Human-readable description of a node for the hover tooltip.
pub(crate) fn node_tooltip(session: &Session, id: NodeId) -> String {
    let board = session.game.board();
    let def = board.definition();
    let node = &def.nodes[id as usize];
    let realm = node.realm.name();
    let coord = node
        .axial
        .map(|ax| format!(" ({},{})", ax[0], ax[1]))
        .unwrap_or_default();
    let occupant = match session.game.state().occupant(id) {
        Some(p) => format!(" — {}", p.name()),
        None => String::new(),
    };
    let origin = def
        .origins
        .iter()
        .find(|o| o.node == id)
        .map(|o| format!(" [{} origin]", o.player.name()))
        .unwrap_or_default();
    let gate = if def.gate_nodes().contains(&id) {
        " [gate]"
    } else {
        ""
    };
    format!("#{id} {realm}{coord}{occupant}{origin}{gate}")
}

pub(crate) fn try_reconnect(commands: &mut Commands, session: &Session, ui: &UiState) {
    if let Connection::Online { room_id, token, .. } = &session.connection {
        let handle = net::connect(&ui.server_addr);
        handle.send(ClientMessage::Reconnect {
            room_id: room_id.clone(),
            token: token.clone(),
        });
        commands.insert_resource(Net(Some(handle)));
    }
}

pub(crate) fn lifeline(groups: u32) -> &'static str {
    match groups {
        1 => "完好",
        2 => "危!",
        _ => "绞杀",
    }
}

pub(crate) fn fmt_clock(ms: u64) -> String {
    let total = ms / 1000;
    format!("{}:{:02}", total / 60, total % 60)
}

/// Tutorial side panel: reads the live game, advances steps, renders text.
pub(crate) fn tutorial_panel(
    mut commands: Commands,
    mut egui_ctx: EguiContexts,
    mut tut: ResMut<Tutorial>,
    session: Res<GameSession>,
    nodes: Query<Entity, With<NodeMarker>>,
    mut ui_state: ResMut<UiState>,
) {
    let game = &session.0.game;
    tut.0.advance(game);
    let (title, body, button) = tut.0.text(game);
    let (idx, total) = tut.0.progress();
    let ctx = egui_ctx.ctx_mut();
    egui::SidePanel::right("tutorial")
        .resizable(false)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(title);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{}/{}", idx.min(total), total));
                });
            });
            ui.add(egui::ProgressBar::new(idx as f32 / total as f32).desired_height(6.0));
            ui.add_space(8.0);
            ui.label(body);
            // Live moment-teaching: the first capture, when it happens.
            let caps = game.state().captures;
            if caps != [0, 0] {
                ui.add_space(6.0);
                let msg = if caps[0] > 0 && caps[1] == 0 {
                    "☠ 你提掉了对方的子——无气之链离场。这就是死亡规则。"
                } else if caps[1] > 0 && caps[0] == 0 {
                    "☠ AI 提掉了你的子！被围死的链会整条消失——记得留气。"
                } else {
                    "☠ 双方都有提子——对杀已经开始。"
                };
                ui.colored_label(egui::Color32::LIGHT_YELLOW, msg);
            }
            ui.add_space(12.0);
            if let Some(label) = button {
                if ui.button(egui::RichText::new(label).strong()).clicked() {
                    if tut.0.step == tutorial::Step::Done {
                        // back to menu: same cleanup as the leave button
                        commands.remove_resource::<Tutorial>();
                        commands.remove_resource::<Active>();
                        commands.remove_resource::<GameSession>();
                        commands.remove_resource::<Net>();
                        commands.remove_resource::<BoardSpawned>();
                        commands.remove_resource::<Palette>();
                        commands.remove_resource::<Replay>();
                        for entity in &nodes {
                            commands.entity(entity).despawn();
                        }
                        ui_state.status.clear();
                    } else {
                        tut.0.next_button();
                    }
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.add_space(8.0);
                if ui.small_button("跳过教程").clicked() {
                    commands.remove_resource::<Tutorial>();
                }
            });
        });
}
