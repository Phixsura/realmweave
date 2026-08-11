//! Session logic: game state mirror + transports. No Bevy types here —
//! the renderer consumes `ViewState` and produces `PlayerIntent`.

use realmweave_core::{BoardGraph, Game, GameConfig, GameResult, Move, NodeId, Player};
use realmweave_protocol::ClockState;

/// What the player wants to do; produced by UI/views, consumed by `Session`.
#[derive(Clone, Debug, PartialEq)]
pub enum PlayerIntent {
    PlaceStone(NodeId),
    SeverStone(NodeId),
    CutEdge(u32),
    Pass,
    Swap,
    Resign,
    /// Local-only: take back the last move (vs-AI: the last exchange).
    Undo,
}

/// Which color(s) this instance controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Local hot-seat: whoever is to move.
    HotSeat,
    /// Online: exactly one color.
    Seat(Player),
    /// Local vs AI: the human plays this color, the bot plays the other.
    VsBot(Player),
    /// AI vs AI exhibition: both colors are bot-driven; no human input.
    BotDuel,
    /// Replay viewer: no input at all.
    Observer,
}

#[derive(Clone, Debug, Default)]
pub enum Connection {
    #[default]
    Local,
    Online {
        room_id: String,
        connected: bool,
        opponent_connected: bool,
        /// Reconnect token for this seat.
        token: String,
    },
}

/// One running game from this client's perspective.
pub struct Session {
    pub game: Game,
    pub control: Control,
    pub connection: Connection,
    pub clock: Option<ClockState>,
    /// Result reported by the server (may include timeout, which the engine
    /// itself cannot produce).
    pub server_result: Option<GameResult>,
    pub last_error: Option<String>,
}

impl Session {
    pub fn hotseat(board: BoardGraph, pie_rule: bool) -> Self {
        Self::hotseat_with_rules(board, pie_rule, realmweave_core::THREE_REALMS_V1)
    }

    #[allow(clippy::unwrap_used, clippy::expect_used)] // construction-time invariants: generated boards validate (CI-gated), live games replay
    pub fn hotseat_with_rules(board: BoardGraph, pie_rule: bool, ruleset: &str) -> Self {
        let config = GameConfig::new(board.definition().id.clone())
            .with_pie_rule(pie_rule)
            .with_ruleset(ruleset);
        let game = Game::new(board, config).expect("valid local game");
        Session {
            game,
            control: Control::HotSeat,
            connection: Connection::Local,
            clock: None,
            server_result: None,
            last_error: None,
        }
    }

    pub fn result(&self) -> Option<GameResult> {
        self.server_result.or(self.game.result())
    }

    /// Did this game's pie swap actually happen? The engine keeps colors
    /// stable on Swap (seats exchange OUTSIDE the engine), and Swap can
    /// only ever be move index 1 — so this is a pure derivation from the
    /// log. Derivation, not mutation: undo, replay, and reconnect all
    /// stay correct for free.
    pub fn swap_happened(&self) -> bool {
        matches!(self.game.state().move_log.get(1), Some(Move::Swap))
    }

    /// The color the human currently plays in VsBot mode — the color
    /// picked at session start, flipped if the pie swap happened. The
    /// single authority: never read the color out of `Control::VsBot`
    /// directly.
    pub fn vs_bot_human(&self) -> Option<Player> {
        match self.control {
            Control::VsBot(p) if self.swap_happened() => Some(p.opponent()),
            Control::VsBot(p) => Some(p),
            _ => None,
        }
    }

    pub fn is_my_turn(&self) -> bool {
        if self.result().is_some() {
            return false;
        }
        match self.control {
            Control::HotSeat => true,
            Control::VsBot(_) => Some(self.game.to_move()) == self.vs_bot_human(),
            Control::Seat(p) => self.game.to_move() == p,
            Control::Observer | Control::BotDuel => false,
        }
    }

    /// The most recently placed node, if the last move was a placement.
    pub fn last_placed(&self) -> Option<NodeId> {
        match self.game.state().move_log.last() {
            Some(Move::Place(n)) => Some(*n),
            _ => None,
        }
    }

    /// The most recently cut edge, if the last move was a cut.
    pub fn last_cut(&self) -> Option<u32> {
        match self.game.state().move_log.last() {
            Some(Move::CutEdge(e)) => Some(*e),
            _ => None,
        }
    }

    /// Human-readable description of the last move (for the HUD banner).
    pub fn last_move_text(&self) -> Option<String> {
        let n = self.game.state().move_log.len();
        n.checked_sub(1).map(|i| self.describe_move(i))
    }

    /// Human-readable description of move `index` in the log.
    /// Light moves first and turns alternate — EXCEPT Swap, which keeps
    /// the mover (Dark swaps, then Dark places): parity inverts for every
    /// move after a swap.
    pub fn describe_move(&self, index: usize) -> String {
        let st = self.game.state();
        let Some(mv) = st.move_log.get(index) else {
            return String::new();
        };
        let flipped = self.swap_happened() && index >= 2;
        let mover = if index.is_multiple_of(2) != flipped {
            Player::Light
        } else {
            Player::Dark
        };
        let bd = self.game.board();
        let is_triforce = self.game.config().ruleset_id == realmweave_core::rules::TRIFORCE_V5;
        let tf_side = realmweave_core::boardgen::tf_side_len(bd.definition());
        let describe_node = |n: NodeId| -> String {
            let node = &bd.definition().nodes[n as usize];
            let ax = node.axial.unwrap_or([0, 0]);
            let region = if is_triforce {
                match realmweave_core::boardgen::triforce_region(bd.definition(), tf_side, n) {
                    0 => "天",
                    1 => "人",
                    2 => "冥",
                    _ => "心",
                }
            } else {
                node.realm.name()
            };
            format!("{region}[{},{}]", ax[0], ax[1])
        };
        match mv {
            Move::Place(n) => format!("{} 落子 {}", mover.name(), describe_node(*n)),
            Move::CutEdge(e) => {
                let edge = &bd.definition().edges[*e as usize];
                format!(
                    "{} ✂ 剪断 {} — {}",
                    mover.name(),
                    describe_node(edge.a),
                    describe_node(edge.b)
                )
            }
            Move::Sever(n) => format!("{} 切除 {}", mover.name(), describe_node(*n)),
            Move::Pass => format!("{} 停一手", mover.name()),
            Move::Swap => "换边".to_string(),
            Move::Resign => format!("{} 认输", mover.name()),
        }
    }

    /// Nodes in components of `player`'s network that contain at least one
    /// origin — the player's live "weave progress".
    pub fn origin_connected(&self, player: Player) -> Vec<NodeId> {
        let origins = self.game.board().definition().origins_of(player);
        self.game
            .player_components(player)
            .into_iter()
            .filter(|c| origins.iter().any(|o| c.binary_search(o).is_ok()))
            .flatten()
            .collect()
    }

    /// Nodes the active player may legally place on (for highlighting).
    pub fn legal_placements(&self) -> Vec<NodeId> {
        if !self.is_my_turn() {
            return Vec::new();
        }
        self.game
            .legal_moves()
            .into_iter()
            .filter_map(|m| match m {
                Move::Place(n) => Some(n),
                _ => None,
            })
            .collect()
    }

    pub fn swap_available(&self) -> bool {
        // Cheap direct check — enumerating every legal move per frame just
        // to find Swap ran a full-board capture simulation each frame.
        let st = self.game.state();
        self.is_my_turn()
            && self.game.config().pie_rule
            && !st.swap_used
            && st.ply == 1
            && st.to_move == Player::Dark
            && self.game.validate(&Move::Swap).is_ok()
    }

    /// Apply an intent locally (hot-seat) — online mode sends to the server
    /// instead and applies on MoveAccepted.
    pub fn apply_local(&mut self, intent: &PlayerIntent) {
        if let PlayerIntent::Undo = intent {
            // vs-AI: rewind to the human's previous decision point (undo
            // the AI's reply too). Hot-seat: one move.
            let times = match self.vs_bot_human() {
                Some(human) if self.game.to_move() == human => 2,
                _ => 1,
            };
            for _ in 0..times {
                if let Err(e) = self.game.undo() {
                    self.last_error = Some(e.to_string());
                    break;
                }
            }
            return;
        }
        let mv = match intent {
            PlayerIntent::PlaceStone(n) => Move::Place(*n),
            PlayerIntent::SeverStone(n) => Move::Sever(*n),
            PlayerIntent::CutEdge(e) => Move::CutEdge(*e),
            PlayerIntent::Pass => Move::Pass,
            PlayerIntent::Swap => Move::Swap,
            PlayerIntent::Resign => Move::Resign,
            PlayerIntent::Undo => unreachable!("handled above"),
        };
        match self.game.play(mv) {
            Ok(_) => self.last_error = None,
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    /// Apply a server-committed move to the local mirror.
    pub fn apply_committed(&mut self, mv: Move) {
        if let Err(e) = self.game.play(mv) {
            // Mirror divergence — should be impossible; surface loudly.
            self.last_error = Some(format!("state desync: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use realmweave_core::boardgen;

    fn vs_bot_pie_game() -> Session {
        let def = boardgen::generate_triforce(10).unwrap();
        let board = BoardGraph::new(def).unwrap();
        let mut s = Session::hotseat_with_rules(board, true, realmweave_core::TRIFORCE_V5);
        s.control = Control::VsBot(Player::Light); // human plays Light
        s
    }

    /// After the AI (Dark) swaps, the human plays Dark: seat exchange is
    /// derived from the log, so undo restores it automatically.
    #[test]
    fn pie_swap_flips_the_vs_bot_seat_and_undo_restores_it() {
        let mut s = vs_bot_pie_game();
        assert_eq!(s.vs_bot_human(), Some(Player::Light));
        s.game.play(Move::Place(26)).unwrap(); // human's strong opening
        assert!(!s.is_my_turn(), "Dark (AI) to move");
        s.game.play(Move::Swap).unwrap(); // AI takes it
        assert!(s.swap_happened());
        assert_eq!(
            s.vs_bot_human(),
            Some(Player::Dark),
            "swap hands the opening stone (Light) to the AI"
        );
        // Engine keeps to_move = Dark after Swap; Dark is now the HUMAN.
        assert!(s.is_my_turn(), "human (now Dark) places next");
        s.game.undo().unwrap(); // pop the Swap
        assert_eq!(
            s.vs_bot_human(),
            Some(Player::Light),
            "derivation rewinds with the log"
        );
    }

    /// Swap keeps the mover, so log parity inverts after it. Index 2 is
    /// Dark's placement, not Light's.
    #[test]
    fn describe_move_attributes_movers_correctly_across_a_swap() {
        let mut s = vs_bot_pie_game();
        s.game.play(Move::Place(26)).unwrap(); // 0: Light
        s.game.play(Move::Swap).unwrap(); // 1: Dark (swap)
        s.game.play(Move::Place(30)).unwrap(); // 2: Dark places
        s.game.play(Move::Place(31)).unwrap(); // 3: Light replies
        assert!(s.describe_move(0).starts_with(Player::Light.name()));
        assert!(s.describe_move(2).starts_with(Player::Dark.name()));
        assert!(s.describe_move(3).starts_with(Player::Light.name()));
        // Without a swap, plain alternation still holds.
        let mut p = vs_bot_pie_game();
        p.game.play(Move::Place(26)).unwrap();
        p.game.play(Move::Place(30)).unwrap();
        assert!(p.describe_move(1).starts_with(Player::Dark.name()));
    }
}
