//! Split from main.rs in the world-class refactor; systems only —
//! shared resources/types stay in `main.rs` (crate root).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::layout::ViewMode;
use crate::net;
use crate::session::{Connection, Control, PlayerIntent, Session};
use crate::{
    tutorial, Active, BoardSpawned, Duel, GameSession, IntentEvent, LocalClocks, Net, NodeMarker,
    Palette, Replay, Tutorial, UiState, ViewSettings,
};
use realmweave_core::{BoardGraph, GameResult, NodeId, Player, WinReason};
use realmweave_protocol::ClientMessage;

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
    clocks: Res<LocalClocks>,
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
            if s.game.rules().uses_scissors() {
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
                // Pass availability is a ruleset capability, NOT an
                // enumeration of every legal move each frame.
                if s.result().is_none()
                    && s.game.rules().allows_pass()
                    && ui.button("pass").clicked()
                {
                    events.send(IntentEvent(PlayerIntent::Pass));
                }
                if s.result().is_none() && ui.button("resign").clicked() {
                    events.send(IntentEvent(PlayerIntent::Resign));
                }
                // Local undo: hot-seat takes back one move; vs-AI takes back
                // the human's move AND the AI's reply. Never online.
                let local_live = matches!(s.connection, Connection::Local)
                    && !matches!(s.control, Control::Observer | Control::BotDuel)
                    && !s.game.state().move_log.is_empty();
                let undo_clicked = local_live && ui.button("↩ 悔棋 (U)").clicked();
                if undo_clicked {
                    view.review_cursor = None;
                    events.send(IntentEvent(PlayerIntent::Undo));
                }
            }
            let charges = s.game.state().sever_charges;
            if charges != [0, 0] {
                ui.label(format!("severs L:{} D:{}", charges[0], charges[1]));
            }
            if view.review_cursor.is_some() {
                ui.separator();
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "复盘中：第 {}/{} 手（←/→ 步进，点棋谱返回）",
                        view.review_cursor.unwrap_or(0),
                        s.game.state().move_log.len()
                    ),
                );
            }
            if view.review_cursor.is_none()
                && s.game.config().ruleset_id == realmweave_core::TRIFORCE_V5
            {
                use realmweave_core::rules::Triforce;
                let (lw, ll) =
                    Triforce::weave_progress(s.game.board(), s.game.state(), Player::Light);
                let (dw, dl) =
                    Triforce::weave_progress(s.game.board(), s.game.state(), Player::Dark);
                ui.label(
                    egui::RichText::new(format!("🕸 织脉 白 {lw}/3 | 黑 {dw}/3"))
                        .strong()
                        .size(16.0),
                );
                // Lone corner stones touch two sides by geometry; only a
                // grown group is a genuine "one side away" threat.
                if s.result().is_none() && lw == 2 && ll >= 4 {
                    ui.colored_label(egui::Color32::GOLD, "白差一边！");
                }
                if s.result().is_none() && dw == 2 && dl >= 4 {
                    ui.colored_label(egui::Color32::LIGHT_RED, "黑差一边！");
                }
            }
            if s.game.config().ruleset_id == realmweave_core::TRINITY_Y_V4 {
                // Per-realm chips: sealed realms show their owner, live
                // realms show who currently leads.
                for (i, name) in ["天", "人", "冥"].iter().enumerate() {
                    let winner = realmweave_core::rules::TrinityY::realm_winner(
                        s.game.board(),
                        s.game.state(),
                        i,
                    );
                    let (text, color) = match winner {
                        Some(Player::Light) => (format!("{name}⛨白"), egui::Color32::GOLD),
                        Some(Player::Dark) => (format!("{name}⛨黑"), egui::Color32::LIGHT_RED),
                        None => (format!("{name}·争"), egui::Color32::GRAY),
                    };
                    ui.colored_label(color, text);
                }
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
            if s.game.rules().uses_scissors() {
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
            if matches!(s.connection, Connection::Local)
                && !matches!(s.control, Control::Observer | Control::BotDuel)
            {
                ui.separator();
                let f = |secs: f32| format!("{}:{:02}", secs as u32 / 60, secs as u32 % 60);
                ui.label(format!(
                    "⏱ 白 {} | 黑 {}",
                    f(clocks.light_s),
                    f(clocks.dark_s)
                ));
            }
            if let Some(text) = s.last_move_text() {
                ui.separator();
                ui.label(egui::RichText::new(text).italics());
            }
            // Illegal-move feedback: suicide/ko/occupied reasons were being
            // recorded and never shown.
            if let Some(err) = &s.last_error {
                ui.separator();
                ui.colored_label(egui::Color32::LIGHT_RED, format!("✕ {err}"));
            }
            if ui.button("leave").clicked() {
                leave_session(&mut commands, &nodes, &mut ui_state);
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
    let realm = if session.game.config().ruleset_id == realmweave_core::TRIFORCE_V5 {
        let side = (((8 * board.node_count() + 1) as f64).sqrt() as usize - 1) / 2;
        match realmweave_core::boardgen::triforce_region(side, id) {
            0 => "Heaven",
            1 => "Mortal",
            2 => "Underworld",
            _ => "Weave-Heart",
        }
    } else {
        node.realm.name()
    };
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
                        leave_session(&mut commands, &nodes, &mut ui_state);
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

/// Game-over modal: result, per-realm outcome, and next actions. Local
/// modes offer save/rematch; online games just surface the result.
#[allow(clippy::too_many_arguments)]
pub(crate) fn game_over_panel(
    mut commands: Commands,
    mut egui_ctx: EguiContexts,
    mut session: ResMut<GameSession>,
    mut ui_state: ResMut<UiState>,
    duel: Option<Res<Duel>>,
    tut: Option<Res<Tutorial>>,
    nodes: Query<Entity, With<NodeMarker>>,
) {
    // The exhibition and tutorial have their own end-of-game flows.
    if duel.is_some() || tut.is_some() {
        return;
    }
    let Some(result) = session.0.result() else {
        return;
    };
    let is_local = matches!(session.0.connection, Connection::Local);
    let ruleset = session.0.game.config().ruleset_id.clone();
    let ctx = egui_ctx.ctx_mut();
    egui::Window::new("对局结束")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, -40.0])
        .show(ctx, |ui| {
            let headline = match result {
                GameResult::Win { player, reason } => {
                    let how = match reason {
                        WinReason::RealmWeave => {
                            if ruleset == realmweave_core::TRINITY_Y_V4 {
                                "取得两界"
                            } else {
                                "编织成网"
                            }
                        }
                        WinReason::Strangle => "绞杀",
                        WinReason::Territory => "领地",
                        WinReason::Resignation => "对方认输",
                        WinReason::Timeout => "对方超时",
                    };
                    format!("{} 获胜 — {}", player.name(), how)
                }
                GameResult::Draw => "平局".to_string(),
            };
            ui.heading(headline);
            ui.add_space(4.0);
            if ruleset == realmweave_core::TRINITY_Y_V4 {
                for (i, name) in ["天界", "人间", "冥界"].iter().enumerate() {
                    let winner = realmweave_core::rules::TrinityY::realm_winner(
                        session.0.game.board(),
                        session.0.game.state(),
                        i,
                    );
                    ui.label(format!(
                        "{name}: {}",
                        winner.map(|p| p.name()).unwrap_or("未分")
                    ));
                }
            }
            if ruleset == realmweave_core::TRIFORCE_V5 {
                if let GameResult::Win { player, .. } = result {
                    // Region composition of the winning position's stones.
                    let bd = session.0.game.board();
                    let st = session.0.game.state();
                    let n = bd.node_count();
                    let side = (((8 * n + 1) as f64).sqrt() as usize - 1) / 2;
                    let mut counts = [0u32; 4];
                    for id in 0..n as realmweave_core::NodeId {
                        if st.occupant(id) == Some(player) {
                            counts[realmweave_core::boardgen::triforce_region(side, id)] += 1;
                        }
                    }
                    ui.label(format!(
                        "胜方布阵：天{} 人{} 冥{} 织心{}",
                        counts[0], counts[1], counts[2], counts[3]
                    ));
                }
            }
            let caps = session.0.game.state().captures;
            if caps != [0, 0] {
                ui.label(format!("提子 白{}:黑{}", caps[0], caps[1]));
            }
            ui.label(format!("共 {} 手", session.0.game.state().move_log.len()));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if is_local && ui.button("💾 保存棋谱").clicked() {
                    let record = session.0.game.record();
                    let path = format!(
                        "game-{}-{}moves.json",
                        session.0.game.board().definition().id,
                        record.moves.len()
                    );
                    match serde_json::to_string_pretty(&record)
                        .map_err(|e| e.to_string())
                        .and_then(|json| std::fs::write(&path, json).map_err(|e| e.to_string()))
                    {
                        Ok(()) => ui_state.status = format!("已保存 {path}"),
                        Err(e) => ui_state.status = format!("保存失败: {e}"),
                    }
                }
                if is_local && ui.button("🔄 再来一局").clicked() {
                    let control = session.0.control;
                    let rules_id = session.0.game.config().ruleset_id.clone();
                    let pie = session.0.game.config().pie_rule;
                    let def = session.0.game.board().definition().clone();
                    let board = BoardGraph::new(def).expect("board round-trips");
                    let mut next = Session::hotseat_with_rules(board, pie, &rules_id);
                    next.control = control;
                    session.0 = next;
                }
                if ui.button("返回菜单").clicked() {
                    leave_session(&mut commands, &nodes, &mut ui_state);
                }
            });
            if !ui_state.status.is_empty() {
                ui.small(&ui_state.status);
            }
        });
}

/// Move-history side panel (toggle: H). Local games only; clicking a move
/// opens the review cursor at that position — read-only, the live game is
/// untouched, and "回到对局" returns to it.
pub(crate) fn history_panel(
    mut egui_ctx: EguiContexts,
    session: Res<GameSession>,
    mut view: ResMut<ViewSettings>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if keys.just_pressed(KeyCode::KeyH) {
        view.show_history = !view.show_history;
    }
    if !view.show_history {
        return;
    }
    let s = &session.0;
    let total = s.game.state().move_log.len();
    let ctx = egui_ctx.ctx_mut();
    egui::SidePanel::left("history")
        .resizable(false)
        .default_width(240.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("棋谱");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{total} 手"));
                });
            });
            ui.separator();
            let review = view.review_cursor;
            egui::ScrollArea::vertical()
                .stick_to_bottom(review.is_none())
                .show(ui, |ui| {
                    for i in 0..total {
                        let text = format!("{:>3}. {}", i + 1, s.describe_move(i));
                        let selected = review == Some(i + 1);
                        if ui.selectable_label(selected, text).clicked() {
                            view.review_cursor = if selected { None } else { Some(i + 1) };
                        }
                    }
                });
            if review.is_some() {
                ui.separator();
                ui.colored_label(egui::Color32::YELLOW, "复盘模式：棋盘显示历史局面");
                if ui.button("▶ 回到对局").clicked() {
                    view.review_cursor = None;
                }
            } else {
                ui.small("点击任意一手进入复盘");
            }
        });
}

/// Tear down the active session completely and return to the menu. The ONE
/// place that knows every session-scoped resource; all "back to menu"
/// buttons go through here (a missed resource here once left a stale
/// Tutorial panel alive across games).
pub(crate) fn leave_session(
    commands: &mut Commands,
    nodes: &Query<Entity, With<NodeMarker>>,
    ui_state: &mut UiState,
) {
    commands.remove_resource::<Tutorial>();
    commands.remove_resource::<Duel>();
    commands.remove_resource::<Active>();
    commands.remove_resource::<GameSession>();
    commands.remove_resource::<Net>();
    commands.remove_resource::<BoardSpawned>();
    commands.remove_resource::<Palette>();
    commands.remove_resource::<Replay>();
    for entity in nodes {
        commands.entity(entity).despawn();
    }
    ui_state.status.clear();
}
