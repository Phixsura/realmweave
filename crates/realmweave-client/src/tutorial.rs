//! Interactive tutorial: a real small-board Trinity Y game vs the bot, with
//! a step panel whose progression is driven by the actual game state — the
//! player learns by doing, not by reading.

use realmweave_core::rules::Triforce;
use realmweave_core::{boardgen, Game, Move, NodeId, Player};

/// Board-level guidance for the current step: nodes and edges to pulse.
#[derive(Default)]
pub struct Hints {
    /// Nodes to highlight.
    pub nodes: Vec<NodeId>,
    /// Edges to highlight (unused by the trinity tutorial; kept for parity).
    pub edges: Vec<u32>,
}

/// How the tutorial bot should behave at this step.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BotMode {
    /// Reading step: the AI waits for you.
    Paused,
    /// Teaching steps: plays simple placements, avoids fights.
    Gentle,
    /// Final step: the real engine.
    Full,
}

/// Which tutorial step is active. Steps auto-advance when the game state
/// satisfies their goal; a few are read-only and advance by button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Orientation: realms, sides, the Y goal.
    Welcome,
    /// Place the first stone.
    FirstStone,
    /// Grow a group that touches two different sides of one realm.
    TouchTwoSides,
    /// The death rule: liberties and capture.
    Death,
    /// Win a realm (complete a Y).
    WinRealm,
    /// Play the match out (two realms win).
    FinishGame,
    /// Wrap-up.
    Done,
}

/// Live tutorial state.
pub struct TutorialState {
    /// Current step.
    pub step: Step,
    /// The color the human plays (always Light in the tutorial).
    pub human: Player,
}

impl TutorialState {
    /// Fresh tutorial at the welcome step.
    pub fn new() -> Self {
        TutorialState {
            step: Step::Welcome,
            human: Player::Light,
        }
    }

    /// The human's moves (Light moves at even move-log indices).
    fn human_moves(&self, game: &Game) -> Vec<Move> {
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

    /// Side length of the tutorial board (one merged triangle).
    fn side(game: &Game) -> usize {
        boardgen::tf_side_len(game.board().definition())
    }

    /// Max number of distinct sides touched by any single human group.
    fn best_group_sides(&self, game: &Game) -> u32 {
        let bd = game.board();
        let st = game.state();
        let side = Self::side(game);
        let n = bd.node_count();
        let mut visited = vec![false; n];
        let mut best = 0u32;
        for start in 0..n as NodeId {
            if st.occupant(start) != Some(self.human) || visited[start as usize] {
                continue;
            }
            let mut stack = vec![start];
            visited[start as usize] = true;
            let mut mask = boardgen::triforce_sides(bd.definition(), side, start);
            while let Some(cur) = stack.pop() {
                for &nb in bd.neighbors(cur) {
                    if !visited[nb as usize] && st.occupant(nb) == Some(self.human) {
                        visited[nb as usize] = true;
                        mask |= boardgen::triforce_sides(bd.definition(), side, nb);
                        stack.push(nb);
                    }
                }
            }
            best = best.max(mask.count_ones());
        }
        best
    }

    /// Advance the step if its goal is met by the current game state.
    /// Returns true when the step just changed.
    pub fn advance(&mut self, game: &Game) -> bool {
        let next = match self.step {
            Step::Welcome => None, // button-driven
            Step::FirstStone => (!self.human_moves(game).is_empty()).then_some(Step::TouchTwoSides),
            Step::TouchTwoSides => (self.best_group_sides(game) >= 2).then_some(Step::Death),
            Step::Death => None, // button-driven
            Step::WinRealm => {
                // Weaving IS winning on the merged board; losing also ends
                // the lesson (result present either way).
                if Triforce::weaver(game.board(), game.state()) == Some(self.human) {
                    Some(Step::Done)
                } else {
                    game.result().is_some().then_some(Step::FinishGame)
                }
            }
            Step::FinishGame => Some(Step::Done),
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
            Step::Death => Step::WinRealm,
            other => other,
        };
    }

    /// (title, body, button_label_if_any) for the current step.
    pub fn text(&self, game: &Game) -> (&'static str, String, Option<&'static str>) {
        match self.step {
            Step::Welcome => (
                "欢迎来到 Realmweave",
                "一整片大三角战场：三个角是天界、人间、冥界，\n\
                 中央交汇之地是「织心」（微微发光的区域）。\n\
                 你执白（发光球体），AI 执黑（棱锥）。\n\n\
                 目标：用一条相连的棋链同时触到大三角的三条边\n\
                 ——编织成网即获胜。数学保证绝无和局。\n\
                 拖动旋转视角，滚轮缩放，V 切 2D 视图。"
                    .to_string(),
                Some("开始 →"),
            ),
            Step::FirstStone => (
                "第一手",
                "点击任意空点落子。\n\
                 建议从一个界域的中腹开始——中心离三条边都近，\n\
                 贴边的直线容易被一刀切断。"
                    .to_string(),
                None,
            ),
            Step::TouchTwoSides => {
                let n = self.best_group_sides(game);
                (
                    "伸向两条边",
                    format!(
                        "把你的棋链延伸到界域的两条不同的边\n\
                         （目前最好的链触到 {n}/2 条）。\n\n\
                         同色相邻自动相连。斜着走（尖）比\n\
                         排直线更有弹性——两条路可以连回来。"
                    ),
                    None,
                )
            }
            Step::Death => (
                "死亡规则",
                "关键规则：一条棋链周围没有任何空点（无气）时，\n\
                 整条链被提离棋盘。\n\n\
                 · 落子先提对方——包围是武器\n\
                 · 让自己无气的落子是禁着（自杀）\n\
                 · 重现之前的整盘局面也是禁着（劫）\n\n\
                 所以长墙需要「眼」（被自己围住的空点），\n\
                 而对方的大链——可以猎杀。"
                    .to_string(),
                Some("明白了 →"),
            ),
            Step::WinRealm => (
                "编织成网",
                "现在完成目标：让一条链同时触到大三角的三条边。\n\
                 每条边横跨两个界域，任何胜利之路都要穿越\n\
                 多个界域、经过织心的争夺。\n\n\
                 注意 AI 也在织。挡它的每一颗子，\n\
                 天然也是你自己 Y 的一部分——攻即是防。"
                    .to_string(),
                None,
            ),
            Step::FinishGame => (
                "终局",
                "拿下第二个界域就是胜利。\n\
                 如果双方各占一界，第三界就是决战场——\n\
                 每一手都在三个战场之间分配时间。"
                    .to_string(),
                None,
            ),
            Step::Done => (
                "教程完成",
                match game.result() {
                    Some(_) => "规则只有两条：编织目标 + 死亡。\n\
                                眼形、劫争、弃子、翻盘——全部从这两条涌现。\n\n\
                                回到菜单，在完整尺寸的棋盘上试试，\n\
                                或者看一场 AI 对弈演示。"
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
            Step::Welcome | Step::Death | Step::Done => BotMode::Paused,
            Step::FirstStone | Step::TouchTwoSides => BotMode::Gentle,
            Step::WinRealm | Step::FinishGame => BotMode::Full,
        }
    }

    /// What to pulse on the board right now.
    pub fn hints(&self, game: &Game) -> Hints {
        let mut h = Hints::default();
        let bd = game.board();
        let st = game.state();
        let side = Self::side(game);
        let n = bd.node_count();
        match self.step {
            Step::Welcome | Step::FirstStone => {
                // pulse the weave-heart: center play beats edge crawling
                for node in 0..n as NodeId {
                    if boardgen::triforce_region(bd.definition(), side, node) == 3
                        && st.occupant(node).is_none()
                    {
                        h.nodes.push(node);
                    }
                }
            }
            Step::TouchTwoSides => {
                // pulse empty side nodes of the big triangle
                for node in 0..n as NodeId {
                    if boardgen::triforce_sides(bd.definition(), side, node) != 0
                        && st.occupant(node).is_none()
                    {
                        h.nodes.push(node);
                    }
                }
            }
            Step::WinRealm | Step::FinishGame | Step::Death | Step::Done => {}
        }
        h
    }

    /// Steps count for the progress display.
    pub fn progress(&self) -> (usize, usize) {
        let idx = match self.step {
            Step::Welcome => 0,
            Step::FirstStone => 1,
            Step::TouchTwoSides => 2,
            Step::Death => 3,
            Step::WinRealm => 4,
            Step::FinishGame => 5,
            Step::Done => 6,
        };
        (idx, 6)
    }
}

impl Default for TutorialState {
    fn default() -> Self {
        Self::new()
    }
}
