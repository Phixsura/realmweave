//! Split from main.rs in the world-class refactor; systems only —
//! shared resources/types stay in `main.rs` (crate root).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

#[cfg(feature = "supplywar-lab")]
use crate::supplywar_ui;
#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use realmweave_core::Move;

pub(crate) fn menu_ui(
    mut commands: Commands,
    mut egui_ctx: EguiContexts,
    mut ui_state: ResMut<UiState>,
) {
    let ctx = egui_ctx.ctx_mut();
    // F1: launch Supply War with the demo AI at the wheel (lab feature).
    #[cfg(feature = "supplywar-lab")]
    if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
        let mut sw = supplywar_ui::SwSession::new(20260809);
        sw.autopilot = true;
        commands.insert_resource(sw);
        commands.insert_resource(Active);
        return;
    }
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading(egui::RichText::new("REALMWEAVE").size(42.0));
            ui.label("weave paths across Heaven, Mortal, and Underworld");
            ui.add_space(30.0);
        });
        let panel_width = 420.0;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(panel_width);
            ui.heading("Board");
            ui.horizontal(|ui| {
                for size in [19usize, 37, 61, 91, 127] {
                    ui.selectable_value(&mut ui_state.board_size, size, format!("{size} × 3"));
                }
            });
            ui.checkbox(&mut ui_state.pie_rule, "pie rule (second player may swap)");
            ui.horizontal(|ui| {
                ui.label("rules");
                for (id, label) in [
                    (realmweave_core::TRINITY_Y_V4, "三界Y (v4)"),
                    (realmweave_core::WEAVE_LAYERS_V3, "层层编织"),
                    (realmweave_core::WEAVE_SEVER_V2, "weave&sever"),
                    (realmweave_core::THREE_REALMS_V1, "classic"),
                    (realmweave_core::SEVER_V1, "sever"),
                ] {
                    ui.selectable_value(&mut ui_state.ruleset, id.to_string(), label);
                }
            });
            ui.add_space(12.0);

            #[cfg(feature = "supplywar-lab")]
            {
                ui.heading("Supply War (prototype)");
                if ui
                    .button("⚡ 开始 Supply War")
                    .on_hover_text("供应线塔防灰盒原型：铺线、防御切割者、封闭裂隙")
                    .clicked()
                {
                    commands.insert_resource(supplywar_ui::SwSession::new(20260809));
                    commands.insert_resource(Active);
                }
                ui.add_space(12.0);
            }

            ui.heading("Local");
            ui.horizontal(|ui| {
                ui.label("world seed (0=classic)");
                let mut seed_str = ui_state.world_seed.to_string();
                if ui.text_edit_singleline(&mut seed_str).changed() {
                    if let Ok(v) = seed_str.parse::<u64>() {
                        ui_state.world_seed = v;
                    }
                }
                if ui.button("🎲").clicked() {
                    // deterministic-ish scramble from the previous value
                    ui_state.world_seed = ui_state
                        .world_seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407)
                        % 100000;
                }
            });
            if ui_state.world_seed != 0 {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "实验：种子世界会挖孔变形，目前偏向绞杀速胜",
                );
            }
            ui.horizontal(|ui| {
                if ui.button("双人热座").clicked() {
                    start_hotseat(
                        &mut commands,
                        ui_state.board_size,
                        ui_state.pie_rule,
                        &ui_state.ruleset.clone(),
                        ui_state.world_seed,
                        None,
                    );
                }
                if realmweave_bot::supports(&ui_state.ruleset) {
                    if ui.button("人机对战 (你执白先手)").clicked() {
                        start_hotseat(
                            &mut commands,
                            ui_state.board_size,
                            ui_state.pie_rule,
                            &ui_state.ruleset.clone(),
                            ui_state.world_seed,
                            Some(Player::Light),
                        );
                    }
                    if ui.button("人机 (你执黑后手)").clicked() {
                        start_hotseat(
                            &mut commands,
                            ui_state.board_size,
                            ui_state.pie_rule,
                            &ui_state.ruleset.clone(),
                            ui_state.world_seed,
                            Some(Player::Dark),
                        );
                    }
                } else {
                    ui.label("(此规则暂无 AI)");
                }
            });
            ui.small(
                "玩法：连接你的三个起源=编织胜 · Tab 切剪线模式(✂×3) · 永久隔离对方起源=绞杀胜",
            );
            if realmweave_bot::supports(&ui_state.ruleset)
                && ui
                    .button("🤖 AI 对弈演示 (慢速讲解 3 局)")
                    .on_hover_text("两个 AI 慢速对弈，每手播报意图")
                    .clicked()
            {
                let def = if ui_state.ruleset == realmweave_core::TRINITY_Y_V4 {
                    boardgen::generate_trinity(14).expect("trinity board")
                } else {
                    boardgen::generate_standard(ui_state.board_size).expect("standard size")
                };
                let board = BoardGraph::new(def).expect("valid board");
                let mut session = Session::hotseat_with_rules(board, false, &ui_state.ruleset);
                session.control = Control::BotDuel;
                commands.insert_resource(GameSession(session));
                commands.insert_resource(Net(None));
                commands.insert_resource(Active);
                commands.insert_resource(Duel {
                    pace: 2.5,
                    timer: 0.0,
                    game_no: 1,
                    games_target: 3,
                    commentary: vec!["—— 第 1 局开始 ——".to_string()],
                    seed: 0xD0E1,
                    next_is_key: false,
                    last_layers: [0, 0],
                    last_captures: [0, 0],
                    board_size: ui_state.board_size,
                    ruleset: ui_state.ruleset.clone(),
                });
            }
            if ui
                .button(egui::RichText::new("📖 新手教程").strong())
                .on_hover_text("小棋盘人机对局，边玩边学（约 5 分钟）")
                .clicked()
            {
                start_hotseat(
                    &mut commands,
                    19,
                    false,
                    realmweave_core::WEAVE_SEVER_V2,
                    0,
                    Some(Player::Light),
                );
                commands.insert_resource(Tutorial(tutorial::TutorialState::new()));
            }
            ui.add_space(12.0);

            ui.heading("Online");
            ui.horizontal(|ui| {
                ui.label("server");
                ui.text_edit_singleline(&mut ui_state.server_addr);
            });
            if ui.button("Create private room").clicked() {
                start_online_create(&mut commands, &mut ui_state);
            }
            ui.horizontal(|ui| {
                ui.label("room code");
                ui.text_edit_singleline(&mut ui_state.room_code);
                if ui.button("Join").clicked() {
                    start_online_join(&mut commands, &mut ui_state);
                }
            });
            ui.add_space(12.0);
            ui.heading("Replay");
            ui.horizontal(|ui| {
                ui.label("record file");
                ui.text_edit_singleline(&mut ui_state.replay_path);
                if ui.button("Open").clicked() {
                    start_replay(&mut commands, &mut ui_state, 0.0);
                }
                if ui.button("Demo (30s/move)").clicked() {
                    start_replay(&mut commands, &mut ui_state, 30.0);
                }
            });
            if !ui_state.status.is_empty() {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::LIGHT_RED, &ui_state.status);
            }
        });
    });
}

pub(crate) fn start_hotseat(
    commands: &mut Commands,
    size: usize,
    pie: bool,
    ruleset: &str,
    world_seed: u64,
    human_vs_bot: Option<Player>,
) {
    let def = if ruleset == realmweave_core::TRINITY_Y_V4 {
        // triangle side from the hex size pick: 19→8, 37→11, 61→14, 91→16, 127→19
        let side = match size {
            19 => 8,
            37 => 11,
            61 => 14,
            91 => 16,
            _ => 19,
        };
        boardgen::generate_trinity(side).expect("trinity board")
    } else if world_seed == 0 {
        boardgen::generate_standard(size).expect("standard size")
    } else {
        boardgen::generate_seeded(size, world_seed).expect("seeded board")
    };
    let board = BoardGraph::new(def).expect("valid board");
    let mut session = Session::hotseat_with_rules(board, pie, ruleset);
    if let Some(human) = human_vs_bot {
        session.control = Control::VsBot(human);
    }
    commands.insert_resource(GameSession(session));
    commands.insert_resource(Net(None));
    commands.insert_resource(Active);
}

pub(crate) fn start_online_create(commands: &mut Commands, ui: &mut UiState) {
    let handle = net::connect(&ui.server_addr);
    let board_id = format!("hex{}-v1", ui.board_size);
    // Local mirror starts from the same generated board; the server snapshot
    // will confirm.
    match net::fetch_board(&ui.server_addr, &board_id)
        .and_then(|def| BoardGraph::new(def).map_err(|e| e.to_string()))
    {
        Ok(board) => {
            let config = realmweave_core::GameConfig::new(board_id)
                .with_pie_rule(ui.pie_rule)
                .with_ruleset(&ui.ruleset)
                .with_time_control(realmweave_core::TimeControl::QUICK);
            handle.send(ClientMessage::CreateRoom {
                config: config.clone(),
            });
            let mut session = Session::hotseat(board, ui.pie_rule);
            session.control = Control::Seat(Player::Light);
            commands.insert_resource(GameSession(session));
            commands.insert_resource(Net(Some(handle)));
            commands.insert_resource(ServerAddr(ui.server_addr.clone()));
            commands.insert_resource(Active);
            ui.status.clear();
        }
        Err(e) => {
            ui.status = format!("cannot reach server: {e}");
        }
    }
}

pub(crate) fn start_replay(commands: &mut Commands, ui: &mut UiState, auto_seconds: f32) {
    match replay::ReplayState::load(&ui.replay_path) {
        Ok(mut state) => {
            state.auto_seconds = auto_seconds;
            state.auto_timer = auto_seconds;
            let initial = state.game_at_cursor();
            match initial {
                Ok(game) => {
                    let board = BoardGraph::new(game.board().definition().clone()).expect("board");
                    let mut session = Session::hotseat(board, false);
                    session.control = Control::Observer;
                    session.game = game;
                    commands.insert_resource(GameSession(session));
                    commands.insert_resource(Net(None));
                    commands.insert_resource(Replay(state));
                    commands.insert_resource(Active);
                    ui.status.clear();
                }
                Err(e) => ui.status = e,
            }
        }
        Err(e) => ui.status = e,
    }
}

pub(crate) fn start_online_join(commands: &mut Commands, ui: &mut UiState) {
    let code = ui.room_code.trim().to_uppercase();
    if code.is_empty() {
        ui.status = "enter a room code".to_string();
        return;
    }
    let handle = net::connect(&ui.server_addr);
    handle.send(ClientMessage::JoinRoom {
        room_id: code.clone(),
    });
    // Placeholder session on the default board; the server Snapshot rebuilds
    // it with the room's real config.
    let board = BoardGraph::new(boardgen::generate_standard(37).unwrap()).unwrap();
    let mut session = Session::hotseat(board, false);
    session.control = Control::Seat(Player::Dark);
    commands.insert_resource(GameSession(session));
    commands.insert_resource(Net(Some(handle)));
    commands.insert_resource(ServerAddr(ui.server_addr.clone()));
    commands.insert_resource(Active);
    ui.status.clear();
}
