//! Interactive tutorial: a real 19×3 game vs the bot, with a step panel
//! whose progression is driven by the actual game state — the player learns
//! by doing, not by reading.

use realmweave_core::{Game, Move, NodeId, Player, Realm};

/// Board-level guidance for the current step: nodes and edges to pulse.
#[derive(Default)]
pub struct Hints {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<u32>,
}

/// How the tutorial bot should behave at this step.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BotMode {
    /// Reading step: the AI waits for you.
    Paused,
    /// Teaching steps: plays simple placements only, never cuts.
    Gentle,
    /// Final step: the real bot.
    Full,
}

/// Which tutorial step is active. Steps auto-advance when the game state
/// satisfies their goal; a few are read-only and advance by button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Welcome,
    FirstStone,
    CrossRealm,
    UseScissors,
    Lifeline,
    FinishGame,
    Done,
}

pub struct TutorialState {
    pub step: Step,
    /// The color the human plays (always Light in the tutorial).
    pub human: Player,
}

impl TutorialState {
    pub fn new() -> Self {
        TutorialState {
            step: Step::Welcome,
            human: Player::Light,
        }
    }

    /// Number of moves the human has made.
    fn human_moves(&self, game: &Game) -> Vec<Move> {
        // Light moves at even move-log indices (strict alternation).
        let par = match self.human {
            Player::Light => 0,
            Player::Dark => 1,
        };
        game.state()
            .move_log
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == par)
            .map(|(_, m)| *m)
            .collect()
    }

    /// Distinct realms where the human has stones.
    fn realms_touched(&self, game: &Game) -> usize {
        let mut seen = [false; 3];
        for (n, occ) in game.state().occupancy.iter().enumerate() {
            if *occ == Some(self.human) {
                let realm = game.board().definition().nodes[n].realm;
                seen[match realm {
                    Realm::Heaven => 0,
                    Realm::Mortal => 1,
                    Realm::Underworld => 2,
                }] = true;
            }
        }
        seen.iter().filter(|&&b| b).count()
    }

    /// Advance the step if its goal is met by the current game state.
    /// Returns true when the step just changed (caller may flash the panel).
    pub fn advance(&mut self, game: &Game) -> bool {
        let next = match self.step {
            Step::Welcome => None, // button-driven
            Step::FirstStone => {
                if !self.human_moves(game).is_empty() {
                    Some(Step::CrossRealm)
                } else {
                    None
                }
            }
            Step::CrossRealm => {
                if self.realms_touched(game) >= 2 {
                    Some(Step::UseScissors)
                } else {
                    None
                }
            }
            Step::UseScissors => {
                if self
                    .human_moves(game)
                    .iter()
                    .any(|m| matches!(m, Move::CutEdge(_)))
                {
                    Some(Step::Lifeline)
                } else {
                    None
                }
            }
            Step::Lifeline => None, // button-driven
            Step::FinishGame => {
                if game.result().is_some() {
                    Some(Step::Done)
                } else {
                    None
                }
            }
            Step::Done => None,
        };
        if let Some(n) = next {
            self.step = n;
            true
        } else {
            false
        }
    }

    /// Button-driven steps: move to the next step explicitly.
    pub fn next_button(&mut self) {
        self.step = match self.step {
            Step::Welcome => Step::FirstStone,
            Step::Lifeline => Step::FinishGame,
            other => other,
        };
    }

    /// (title, body, button_label_if_any) for the current step.
    pub fn text(&self, game: &Game) -> (&'static str, String, Option<&'static str>) {
        match self.step {
            Step::Welcome => (
                "欢迎来到 Realmweave",
                "棋盘有三层界域：天界、人间、冥界。\n\
                 你执白（发光球体），AI 执黑（棱锥）。\n\
                 你在每层各有一个「起源」（高亮节点）。\n\n\
                 目标：把三个起源连成一张网 = 编织胜。\n\n\
                 拖动鼠标旋转视角，滚轮缩放，V 切换 2D 分析视图。"
                    .to_string(),
                Some("开始 →"),
            ),
            Step::FirstStone => (
                "第一手：落子",
                "点击任意空节点放下一颗棋子。\n\
                 建议下在自己起源附近——你的网要从起源长出来。\n\
                 同色相邻的棋子自动连通。"
                    .to_string(),
                None,
            ),
            Step::CrossRealm => {
                let n = self.realms_touched(game);
                (
                    "跨越界域",
                    format!(
                        "三个起源分别在三层，光靠一层连不成网。\n\
                         紫色竖线是「门」——唯一能穿越界域的通道。\n\
                         门很稀少，是全盘必争之地。\n\n\
                         目标：在第二层界域落子（已触及 {n}/2 层）。\n\
                         提示：先占住一根门柱的端点。"
                    ),
                    None,
                )
            }
            Step::UseScissors => (
                "剪刀：改变世界",
                "你有 3 把剪刀（HUD 上的 ✂）。剪断一条边，它就永远消失——\n\
                 不是挡路，是把路从世界上抹掉。\n\n\
                 按 Tab 进入剪线模式，依次点击一条边的两个端点。\n\
                 剪 AI 棋子旁边的边，断它的连接。\n\
                 （起源紧邻的边受保护，剪不断。）"
                    .to_string(),
                None,
            ),
            Step::Lifeline => (
                "绞杀：另一条胜路",
                "剪刀带来第二种胜利：如果对手的三个起源被剪得\n\
                 「永久无法互连」（无论后面怎么下都连不上），\n\
                 它就被绞杀，你直接获胜。\n\n\
                 HUD 的「生命线」显示双方起源的连通健康度；\n\
                 出现红色 ⚠ 表示有人的起源已被割裂。\n\
                 注意：让自己被割裂的剪法是禁着。"
                    .to_string(),
                Some("明白了 →"),
            ),
            Step::FinishGame => (
                "终局",
                "把这局下完：连通三个起源并挺过 AI 一回合（编织胜），\n\
                 或者用剪刀绞杀它。\n\
                 卡住时看 HUD 的最后一手播报，理解 AI 的意图。"
                    .to_string(),
                None,
            ),
            Step::Done => (
                "教程完成",
                match game.result() {
                    Some(_) => "这一局结束了。规则只有落子和剪线两个动作，\n\
                                但每一刀都在改写棋盘本身——\n\
                                所以每一局都是新棋局。\n\n\
                                回到菜单，试试 61×3 标准盘。"
                        .to_string(),
                    None => String::new(),
                },
                Some("返回菜单"),
            ),
        }
    }

    /// Bot behavior for the current step.
    pub fn bot_mode(&self) -> BotMode {
        match self.step {
            Step::Welcome | Step::Lifeline | Step::Done => BotMode::Paused,
            Step::FirstStone | Step::CrossRealm | Step::UseScissors => BotMode::Gentle,
            Step::FinishGame => BotMode::Full,
        }
    }

    /// What to pulse on the board right now.
    pub fn hints(&self, game: &Game) -> Hints {
        let mut h = Hints::default();
        let bd = game.board();
        let st = game.state();
        match self.step {
            Step::Welcome => {
                // show your origins
                h.nodes = bd.definition().origins_of(self.human);
            }
            Step::FirstStone => {
                // empties adjacent to your origins
                for &o in &bd.definition().origins_of(self.human) {
                    h.nodes.push(o);
                    for nb in bd.live_neighbors(o, &st.cut_edges) {
                        if st.occupant(nb).is_none() {
                            h.nodes.push(nb);
                        }
                    }
                }
            }
            Step::CrossRealm => {
                // gate endpoints in realms you haven't reached, plus the
                // gate columns themselves
                let mut reached = [false; 3];
                for (n, occ) in st.occupancy.iter().enumerate() {
                    if *occ == Some(self.human) {
                        reached[bd.definition().nodes[n].realm as usize] = true;
                    }
                }
                for (ei, e) in bd.definition().edges.iter().enumerate() {
                    if e.kind == realmweave_core::EdgeKind::Portal
                        && !st.cut_edges.contains(&(ei as u32))
                    {
                        h.edges.push(ei as u32);
                        for node in [e.a, e.b] {
                            let r = bd.definition().nodes[node as usize].realm as usize;
                            if !reached[r] && st.occupant(node).is_none() {
                                h.nodes.push(node);
                            }
                        }
                    }
                }
            }
            Step::UseScissors => {
                // cuttable edges touching an AI stone
                let opp = self.human.opponent();
                for (ei, e) in bd.definition().edges.iter().enumerate() {
                    let ei = ei as u32;
                    if st.cut_edges.contains(&ei) {
                        continue;
                    }
                    if (st.occupant(e.a) == Some(opp) || st.occupant(e.b) == Some(opp))
                        && game.validate(&Move::CutEdge(ei)).is_ok()
                    {
                        h.edges.push(ei);
                    }
                }
            }
            Step::Lifeline | Step::FinishGame | Step::Done => {}
        }
        h
    }

    /// Steps count for the progress display (Done excluded).
    pub fn progress(&self) -> (usize, usize) {
        let idx = match self.step {
            Step::Welcome => 0,
            Step::FirstStone => 1,
            Step::CrossRealm => 2,
            Step::UseScissors => 3,
            Step::Lifeline => 4,
            Step::FinishGame => 5,
            Step::Done => 6,
        };
        (idx, 6)
    }
}
