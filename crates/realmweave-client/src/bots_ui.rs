//! Split from main.rs in the world-class refactor; systems only —
//! shared resources/types stay in `main.rs` (crate root).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use realmweave_core::Move;

/// Bot turn driver: when control is VsBot and it's the bot's color to move,
/// compute a move (blocking is fine at this bot's speed: <1s typical) after
/// a short human-feeling delay.
pub(crate) fn bot_turn(
    time: Res<Time>,
    mut think: Local<f32>,
    mut session: ResMut<GameSession>,
    tut: Option<Res<Tutorial>>,
) {
    let s = &mut session.0;
    let Control::VsBot(human) = s.control else {
        *think = 0.0;
        return;
    };
    if s.result().is_some() || s.game.to_move() == human {
        *think = 0.0;
        return;
    }
    // Tutorial pacing: pause while the player reads; play gently while
    // they learn the verbs; full strength only in the final step.
    let mode = tut
        .map(|t| t.0.bot_mode())
        .unwrap_or(tutorial::BotMode::Full);
    if mode == tutorial::BotMode::Paused {
        *think = 0.0;
        return;
    }
    *think += time.delta_secs();
    if *think < 0.6 {
        return; // brief pause so moves feel deliberate
    }
    *think = 0.0;
    let seed = 0xB07 ^ (s.game.state().ply as u64).wrapping_mul(2654435761);
    let mv = match mode {
        tutorial::BotMode::Gentle => gentle_bot_move(&s.game, seed),
        _ => realmweave_bot::choose_move(&s.game, seed),
    };
    if let Some(mv) = mv {
        let _ = s.game.play(mv);
    } else {
        // no candidate — pass if possible, else resign is never auto-played
        let _ = s.game.play(realmweave_core::Move::Pass);
    }
}

/// Tutorial sparring: placements only (never cuts), grown from the bot's
/// own stones/origins so it builds a visible, readable shape — and never
/// blocks the player's teaching goals aggressively.
pub(crate) fn gentle_bot_move(game: &Game, seed: u64) -> Option<Move> {
    let bd = game.board();
    let st = game.state();
    let me = game.to_move();
    let mut anchors: Vec<NodeId> = bd.definition().origins_of(me);
    for (n, occ) in st.occupancy.iter().enumerate() {
        if *occ == Some(me) {
            anchors.push(n as NodeId);
        }
    }
    let mut cands: Vec<NodeId> = Vec::new();
    for &a in &anchors {
        for nb in bd.live_neighbors(a, &st.cut_edges) {
            if st.occupant(nb).is_none() && game.validate(&Move::Place(nb)).is_ok() {
                cands.push(nb);
            }
        }
    }
    cands.sort_unstable();
    cands.dedup();
    if cands.is_empty() {
        return realmweave_bot::choose_move(game, seed);
    }
    let pick = (seed as usize).wrapping_add(st.ply as usize * 7) % cands.len();
    Some(Move::Place(cands[pick]))
}

/// AI-vs-AI exhibition driver: play one move per `pace` seconds with
/// narrated reasoning; start the next game when one ends.
pub(crate) fn duel_turn(
    time: Res<Time>,
    mut commands: Commands,
    duel: Option<ResMut<Duel>>,
    mut session: ResMut<GameSession>,
) {
    let Some(mut duel) = duel else { return };
    let s = &mut session.0;
    if s.control != Control::BotDuel {
        return;
    }
    duel.timer += time.delta_secs();
    // Variable pacing: linger on big moves (cuts, weave threats), breeze
    // through quiet development moves.
    let wait = if duel.next_is_key {
        duel.pace * 2.0
    } else {
        duel.pace
    };
    if duel.timer < wait {
        return;
    }
    duel.timer = 0.0;

    // Game over → linger one beat, then next game or stop.
    if let Some(result) = s.result() {
        let verdict = match result {
            GameResult::Win { player, reason } => format!(
                "第 {} 局结束：{} 获胜（{}），共 {} 手",
                duel.game_no,
                player.name(),
                win_reason_name(reason),
                s.game.state().move_log.len()
            ),
            GameResult::Draw => format!("第 {} 局结束：平局", duel.game_no),
        };
        push_commentary(&mut duel.commentary, verdict);
        if duel.game_no >= duel.games_target {
            push_commentary(
                &mut duel.commentary,
                "对弈结束。点 leave 返回菜单。".to_string(),
            );
            return;
        }
        duel.game_no += 1;
        let def = if duel.ruleset == realmweave_core::TRINITY_Y_V4 {
            boardgen::generate_trinity(14).expect("trinity board")
        } else {
            boardgen::generate_standard(duel.board_size).expect("standard size")
        };
        let board = BoardGraph::new(def).expect("valid board");
        let mut next = Session::hotseat_with_rules(board, false, &duel.ruleset.clone());
        next.control = Control::BotDuel;
        let opener = format!("—— 第 {} 局开始 ——", duel.game_no);
        push_commentary(&mut duel.commentary, opener);
        duel.last_layers = [0, 0];
        duel.last_captures = [0, 0];
        *s = next;
        return;
    }

    let mover = s.game.to_move();
    // Health before the move, both sides, for narration.
    let my_before = realmweave_bot::link_cost(&s.game, mover);
    let opp_before = realmweave_bot::link_cost(&s.game, mover.opponent());
    let seed = duel
        .seed
        .wrapping_add(duel.game_no as u64 * 0x9E37)
        .wrapping_add(s.game.state().ply as u64);
    let mv = realmweave_bot::choose_move(&s.game, seed).unwrap_or(realmweave_core::Move::Pass);
    if s.game.play(mv).is_err() {
        let _ = s.game.play(realmweave_core::Move::Pass);
        return;
    }
    let my_after = realmweave_bot::link_cost(&s.game, mover);
    let opp_after = realmweave_bot::link_cost(&s.game, mover.opponent());
    let mut line = s.last_move_text().unwrap_or_default();
    // Intent language, not raw numbers: classify by relative effect size.
    let d_mine = my_before - my_after; // + = my position improved
    let d_opp = opp_after - opp_before; // + = opponent hurt
    let why = match mv {
        Move::CutEdge(_) => {
            if d_opp >= 8 {
                "——致命一剪：对方的主干路线断了".to_string()
            } else if d_opp > 0 {
                "——骚扰性剪断，逼对方绕路".to_string()
            } else {
                "——预防性剪断，先拆掉将来会被利用的桥".to_string()
            }
        }
        Move::Place(_) => {
            if d_mine > 0 && d_opp > 0 {
                "——一子两用：既延伸自己的网，又挡住对方要道".to_string()
            } else if d_mine >= 4 {
                "——关键连接：两片棋连上了".to_string()
            } else if d_mine > 0 {
                "——铺网，向下一个起源推进".to_string()
            } else if d_opp >= 4 {
                "——强硬拦截，封住对方必经之路".to_string()
            } else if d_opp > 0 {
                "——试探性挡子".to_string()
            } else {
                "——补形，给自己的网留后路".to_string()
            }
        }
        _ => String::new(),
    };
    line.push_str(&why);
    // Key-move detection for pacing: cuts, big swings, weave threats.
    duel.next_is_key = matches!(mv, Move::CutEdge(_))
        || d_mine >= 4
        || d_opp >= 4
        || s.game.state().pending_weave.is_some();
    if s.game.state().pending_weave.is_some() {
        line.push_str(" 🕸 编织成形——下一手是生死劫！");
    }
    push_commentary(&mut duel.commentary, line);
    // Capture happened? (trinity death rule)
    let caps_now = s.game.state().captures;
    if caps_now != duel.last_captures {
        let (who, n) = if caps_now[0] > duel.last_captures[0] {
            (Player::Light, caps_now[0] - duel.last_captures[0])
        } else {
            (Player::Dark, caps_now[1] - duel.last_captures[1])
        };
        push_commentary(
            &mut duel.commentary,
            format!("☠ {} 提掉对方 {n} 子——无气之链离场。", who.name()),
        );
        duel.next_is_key = true;
        duel.last_captures = caps_now;
    }
    // Layer scored this move? (layers changed = petrification happened)
    let layers_now = s.game.state().layers;
    if layers_now != duel.last_layers {
        let who = if layers_now[0] > duel.last_layers[0] {
            Player::Light
        } else {
            Player::Dark
        };
        let line = if s.game.config().ruleset_id == realmweave_core::TRINITY_Y_V4 {
            format!(
                "⚖ {} 织成一个界域（{}:{}）！该界封印，战火转移到其余界域。",
                who.name(),
                layers_now[0],
                layers_now[1]
            )
        } else {
            format!(
                "⛰ {} 织成第 {} 层！整张网固化成世界结构，双方剪刀补给 +2，棋局在变形后的世界继续。",
                who.name(),
                layers_now[player_idx(who)]
            )
        };
        push_commentary(&mut duel.commentary, line);
        duel.next_is_key = true;
        duel.last_layers = layers_now;
    }
    let _ = &mut commands; // reserved for future effects
}

pub(crate) fn push_commentary(log: &mut Vec<String>, line: String) {
    log.push(line);
    let overflow = log.len().saturating_sub(8);
    if overflow > 0 {
        log.drain(..overflow);
    }
}

pub(crate) fn player_idx(p: Player) -> usize {
    match p {
        Player::Light => 0,
        Player::Dark => 1,
    }
}

pub(crate) fn win_reason_name(reason: WinReason) -> &'static str {
    match reason {
        WinReason::RealmWeave => "编织成网",
        WinReason::Strangle => "绞杀",
        WinReason::Territory => "领地",
        WinReason::Resignation => "认输",
        WinReason::Timeout => "超时",
    }
}

/// Duel commentary panel: shows the exhibition's rolling narration.
pub(crate) fn duel_panel(mut egui_ctx: EguiContexts, duel: Res<Duel>, session: Res<GameSession>) {
    let ctx = egui_ctx.ctx_mut();
    egui::SidePanel::right("duel")
        .resizable(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading(format!(
                "AI 对弈 · 第 {}/{} 局 · 第 {} 手",
                duel.game_no,
                duel.games_target,
                session.0.game.state().move_log.len()
            ));
            ui.add_space(6.0);
            for line in &duel.commentary {
                ui.label(line);
                ui.add_space(2.0);
            }
        });
}
