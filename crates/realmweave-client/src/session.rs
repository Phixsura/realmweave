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

    pub fn is_my_turn(&self) -> bool {
        if self.result().is_some() {
            return false;
        }
        match self.control {
            Control::HotSeat => true,
            Control::Seat(p) | Control::VsBot(p) => self.game.to_move() == p,
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
        let st = self.game.state();
        let mv = st.move_log.last()?;
        // Whose move was it? Light moves first and turns strictly alternate
        // (Pass and Swap included), so move-index parity is authoritative.
        let mover = if (st.move_log.len() - 1).is_multiple_of(2) {
            Player::Light
        } else {
            Player::Dark
        };
        let bd = self.game.board();
        let describe_node = |n: NodeId| -> String {
            let node = &bd.definition().nodes[n as usize];
            let ax = node.axial.unwrap_or([0, 0]);
            format!("{}[{},{}]", node.realm.name(), ax[0], ax[1])
        };
        Some(match mv {
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
        })
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
        self.is_my_turn() && self.game.legal_moves().contains(&Move::Swap)
    }

    /// Apply an intent locally (hot-seat) — online mode sends to the server
    /// instead and applies on MoveAccepted.
    pub fn apply_local(&mut self, intent: &PlayerIntent) {
        if let PlayerIntent::Undo = intent {
            // vs-AI: rewind to the human's previous decision point (undo
            // the AI's reply too). Hot-seat: one move.
            let times = match self.control {
                Control::VsBot(human) => {
                    if self.game.to_move() == human {
                        2
                    } else {
                        1
                    }
                }
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
