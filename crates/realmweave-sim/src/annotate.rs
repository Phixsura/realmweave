//! Generate per-move commentary for a recorded game by measuring what each
//! move changed: connection distances, captures, territory, weave status.
//! Output is a sidecar JSON consumed by the client's replay viewer.

#![allow(clippy::unwrap_used, clippy::expect_used)] // offline tooling: fail fast is correct
use realmweave_core::{BoardGraph, Game, GameConfig, Move, NodeId, Player, Realm};
use serde::Serialize;

#[derive(Serialize)]
pub struct Annotation {
    /// 1-based ply this text describes.
    pub ply: u32,
    pub player: String,
    pub text: String,
}

fn realm_name(realm: Realm) -> &'static str {
    match realm {
        Realm::Heaven => "天界",
        Realm::Mortal => "人间",
        Realm::Underworld => "冥界",
    }
}

fn node_label(board: &BoardGraph, node: NodeId) -> String {
    let def = &board.definition().nodes[node as usize];
    let ax = def.axial.unwrap_or([0, 0]);
    let gate = if board.definition().gate_nodes().contains(&node) {
        "·门"
    } else {
        ""
    };
    format!("{}[{},{}]{}", realm_name(def.realm), ax[0], ax[1], gate)
}

/// Own-network health: sum over origin pairs of 0/1-BFS distance where own
/// stones cost 0 and empty nodes cost 1 (enemy stones impassable).
fn connection_cost(game: &Game, player: Player) -> i64 {
    let board = game.board();
    let origins = board.definition().origins_of(player);
    let mut total = 0i64;
    for i in 0..origins.len() {
        for j in (i + 1)..origins.len() {
            total += zero_one(game, origins[i], origins[j], player).unwrap_or(200);
        }
    }
    total
}

fn zero_one(game: &Game, from: NodeId, to: NodeId, player: Player) -> Option<i64> {
    let board = game.board();
    let state = game.state();
    let n = board.node_count();
    let mut dist = vec![i64::MAX; n];
    let mut deque = std::collections::VecDeque::new();
    dist[from as usize] = 0;
    deque.push_back(from);
    while let Some(cur) = deque.pop_front() {
        if cur == to {
            return Some(dist[cur as usize]);
        }
        for &next in board.neighbors(cur) {
            let cost = match state.occupant(next) {
                Some(p) if p == player => 0,
                None => 1,
                _ => continue,
            };
            let nd = dist[cur as usize] + cost;
            if nd < dist[next as usize] {
                dist[next as usize] = nd;
                if cost == 0 {
                    deque.push_front(next);
                } else {
                    deque.push_back(next);
                }
            }
        }
    }
    None
}

/// Produce annotations for every move of a recorded game (generic across
/// rulesets; wording is connection-flavored).
pub fn annotate(board: BoardGraph, config: GameConfig, moves: &[Move]) -> Vec<Annotation> {
    let mut game = Game::new(BoardGraph::new(board.definition().clone()).unwrap(), config).unwrap();
    let mut out = Vec::new();

    for (i, mv) in moves.iter().enumerate() {
        let mover = game.to_move();
        let me_before = connection_cost(&game, mover);
        let them_before = connection_cost(&game, mover.opponent());
        let my_weave_before = game.has_realm_weave(mover);
        let caps_before = game.state().captures;

        game.play(*mv).expect("record replays");

        let me_after = connection_cost(&game, mover);
        let them_after = connection_cost(&game, mover.opponent());
        let caps_after = game.state().captures;
        let board_ref = game.board();

        let who = match mover {
            Player::Light => "白(Light)",
            Player::Dark => "黑(Dark)",
        };
        let mut text = match mv {
            Move::Place(node) => {
                let mut parts: Vec<String> = Vec::new();
                let captured = (caps_after[0] + caps_after[1]) - (caps_before[0] + caps_before[1]);
                if captured > 0 {
                    parts.push(format!("提掉对方 {captured} 子！无气之链整组离场"));
                }
                let my_gain = me_before - me_after;
                if my_gain > 0 {
                    parts.push(format!("己方三起源连接距离缩短 {my_gain} 步"));
                }
                let their_loss = them_after - them_before;
                if their_loss > 0 {
                    parts.push(format!("同时把对方的连接路线逼远 {their_loss} 步"));
                }
                if !my_weave_before && game.has_realm_weave(mover) {
                    parts.push("三起源贯通——编织完成".to_string());
                }
                if parts.is_empty() {
                    parts.push(if board_ref.definition().gate_nodes().contains(node) {
                        "抢占门柱：这是穿层电梯口，为跨界连接预留通道".to_string()
                    } else {
                        "铺网／扩张势力范围的次序棋".to_string()
                    });
                }
                format!(
                    "落子 {}。{}",
                    node_label(board_ref, *node),
                    parts.join("；")
                )
            }
            Move::Pass => "停一手".to_string(),
            Move::Sever(node) => format!("切断 {}", node_label(board_ref, *node)),
            Move::CutEdge(e) => {
                let edge = &board_ref.definition().edges[*e as usize];
                format!(
                    "剪线！{} — {} 之间的通路被永久切断",
                    node_label(board_ref, edge.a),
                    node_label(board_ref, edge.b)
                )
            }
            Move::Swap => "换边（pie rule）".to_string(),
            Move::Resign => "认输".to_string(),
        };
        // Capture-count line every 10 moves for orientation.
        if (i + 1) % 10 == 0 {
            text.push_str(&format!(
                "　【第{}手：提子 白{}:黑{}】",
                i + 1,
                game.state().captures[0],
                game.state().captures[1]
            ));
        }
        out.push(Annotation {
            ply: (i + 1) as u32,
            player: who.to_string(),
            text,
        });
    }
    out
}
