//! Room state machine: two seats, one authoritative `Game`, server clocks.
//!
//! All mutation happens through `Room::handle_*` methods while holding the
//! room lock; results are broadcast through per-seat channels.

use std::time::Instant;

use realmweave_core::{Game, GameResult, Move, Player, TimeControl, WinReason};
use realmweave_protocol::{ClockState, GameSnapshot, MoveEvent, ServerMessage};
use tokio::sync::mpsc;

/// Per-seat outbound message channel.
pub type Tx = mpsc::UnboundedSender<ServerMessage>;

/// One player's seat: reconnect token + live channel (None = disconnected).
pub struct Seat {
    /// Reconnect token for this seat.
    pub token: String,
    /// Outbound channel while connected.
    pub tx: Option<Tx>,
}

/// One live match: two seats, one authoritative game, server clocks.
pub struct Room {
    /// Public room code.
    pub id: String,
    /// Persistent game id (storage key).
    pub game_id: String,
    /// The authoritative game.
    pub game: Game,
    /// Clock settings, if any.
    pub time_control: Option<TimeControl>,
    /// Light's remaining bank, ms.
    pub light_ms: u64,
    /// Dark's remaining bank, ms.
    pub dark_ms: u64,
    /// When the running clock started counting, if the game is live.
    pub turn_started: Option<Instant>,
    /// Light seat (room creator).
    pub light: Seat,
    /// Dark seat once someone joins.
    pub dark: Option<Seat>,
    /// Canonical room event counter.
    pub event_seq: u64,
    /// Both seats filled and play begun.
    pub started: bool,
    /// Game over (engine result or override).
    pub finished: bool,
    /// Result decided outside the rules engine (timeout).
    pub result_override: Option<GameResult>,
    /// Last connect/move/disconnect activity (for the reaper).
    pub last_activity: Instant,
}

impl Room {
    /// Whether both seats are currently disconnected.
    pub fn fully_disconnected(&self) -> bool {
        self.light.tx.is_none() && self.dark.as_ref().map(|d| d.tx.is_none()).unwrap_or(true)
    }

    /// Fresh room with Light seated.
    pub fn new(id: String, game_id: String, game: Game, light_token: String) -> Self {
        let time_control = game.config().time_control;
        let base = time_control.map(|tc| tc.base_ms).unwrap_or(0);
        Room {
            id,
            game_id,
            game,
            time_control,
            light_ms: base,
            dark_ms: base,
            turn_started: None,
            light: Seat {
                token: light_token,
                tx: None,
            },
            dark: None,
            event_seq: 0,
            started: false,
            finished: false,
            result_override: None,
            last_activity: Instant::now(),
        }
    }

    /// Canonical result: engine result, or a server-decided override.
    pub fn result(&self) -> Option<GameResult> {
        self.game.result().or(self.result_override)
    }

    /// Allocate the next canonical event number.
    pub fn next_seq(&mut self) -> u64 {
        self.event_seq += 1;
        self.event_seq
    }

    /// Which seat a reconnect token belongs to.
    pub fn seat_of_token(&self, token: &str) -> Option<Player> {
        if self.light.token == token {
            return Some(Player::Light);
        }
        if let Some(dark) = &self.dark {
            if dark.token == token {
                return Some(Player::Dark);
            }
        }
        None
    }

    /// Mutable seat access.
    pub fn seat_mut(&mut self, player: Player) -> Option<&mut Seat> {
        match player {
            Player::Light => Some(&mut self.light),
            Player::Dark => self.dark.as_mut(),
        }
    }

    /// Send to one seat if connected (silently drops otherwise).
    pub fn send_to(&self, player: Player, msg: ServerMessage) {
        let seat = match player {
            Player::Light => Some(&self.light),
            Player::Dark => self.dark.as_ref(),
        };
        if let Some(Seat { tx: Some(tx), .. }) = seat {
            let _ = tx.send(msg);
        }
    }

    /// Send to both seats.
    pub fn broadcast(&self, msg: ServerMessage) {
        self.send_to(Player::Light, msg.clone());
        self.send_to(Player::Dark, msg);
    }

    /// Current clock state, accounting for elapsed time on the running side.
    pub fn clock(&self) -> ClockState {
        let mut light_ms = self.light_ms;
        let mut dark_ms = self.dark_ms;
        let running = if self.started && !self.finished {
            let mover = self.game.to_move();
            if let Some(t0) = self.turn_started {
                let elapsed = t0.elapsed().as_millis() as u64;
                match mover {
                    Player::Light => light_ms = light_ms.saturating_sub(elapsed),
                    Player::Dark => dark_ms = dark_ms.saturating_sub(elapsed),
                }
            }
            Some(mover)
        } else {
            None
        };
        ClockState {
            light_ms,
            dark_ms,
            running,
        }
    }

    /// Commit elapsed time for the player who just moved, apply increment.
    fn settle_clock(&mut self, mover: Player) {
        if self.time_control.is_none() {
            return;
        }
        let increment = self.time_control.map(|tc| tc.increment_ms).unwrap_or(0);
        if let Some(t0) = self.turn_started.take() {
            let elapsed = t0.elapsed().as_millis() as u64;
            let remaining = match mover {
                Player::Light => &mut self.light_ms,
                Player::Dark => &mut self.dark_ms,
            };
            *remaining = remaining.saturating_sub(elapsed).saturating_add(increment);
        }
        if !self.finished {
            self.turn_started = Some(Instant::now());
        }
    }

    /// Has the running player's flag fallen?
    pub fn flagged(&self) -> Option<Player> {
        if !self.started || self.finished || self.time_control.is_none() {
            return None;
        }
        let clock = self.clock();
        let mover = self.game.to_move();
        (clock.remaining(mover) == 0).then_some(mover)
    }

    /// Full state snapshot for (re)joining clients.
    pub fn snapshot_for(&self, seat: Player) -> GameSnapshot {
        let opponent_connected = match seat.opponent() {
            Player::Light => self.light.tx.is_some(),
            Player::Dark => self.dark.as_ref().map(|s| s.tx.is_some()).unwrap_or(false),
        };
        GameSnapshot {
            config: self.game.config().clone(),
            moves: self.game.state().move_log.clone(),
            clock: self.clock(),
            seat,
            opponent_connected,
            started: self.started,
            result: self.result(),
        }
    }

    /// Start the match once both seats are filled. Returns true on start.
    pub fn start_if_ready(&mut self) -> bool {
        if !self.started && self.dark.is_some() {
            self.started = true;
            if self.time_control.is_some() {
                self.turn_started = Some(Instant::now());
            }
            return true;
        }
        false
    }

    /// Validate and apply a move from `seat`. On success returns the event
    /// to broadcast; the caller persists it.
    pub fn play(&mut self, seat: Player, mv: Move) -> Result<MoveEvent, String> {
        if !self.started {
            return Err("game has not started".to_string());
        }
        if self.finished {
            return Err("game is finished".to_string());
        }
        // Flag-fall is checked HERE, not only by the 1-second ticker: a
        // move landing inside the tick window after the bank hit zero
        // would otherwise be accepted — and with increment it would even
        // REVIVE the flagged player (0 − elapsed saturates to 0, then
        // +increment), making time losses unenforceable.
        if self.flagged() == Some(self.game.to_move()) {
            return Err("flag fell".to_string());
        }
        // Resignation is legal at any time (like chess). The ENGINE models
        // Resign as "the mover resigns", so an off-turn resignation must be
        // adjudicated by the server instead of routed through the engine
        // (which would record the WRONG player as resigning).
        if mv == Move::Resign && self.game.to_move() != seat {
            self.finished = true;
            self.turn_started = None;
            self.result_override = Some(GameResult::Win {
                player: seat.opponent(),
                reason: WinReason::Resignation,
            });
            let seq = self.next_seq();
            return Ok(MoveEvent {
                seq,
                ply: self.game.state().ply,
                player: seat,
                mv,
                clock: self.clock(),
            });
        }
        if self.game.to_move() != seat {
            return Err("not your turn".to_string());
        }
        self.game.validate(&mv).map_err(|e| e.to_string())?;
        self.game.play(mv).map_err(|e| e.to_string())?;
        if self.game.result().is_some() {
            self.finished = true;
        }
        self.settle_clock(seat);
        let seq = self.next_seq();
        Ok(MoveEvent {
            seq,
            ply: self.game.state().ply,
            player: seat,
            mv,
            clock: self.clock(),
        })
    }

    /// End the game by flag fall.
    pub fn timeout(&mut self, flagged: Player) -> GameResult {
        self.finished = true;
        self.turn_started = None;
        match flagged {
            Player::Light => self.light_ms = 0,
            Player::Dark => self.dark_ms = 0,
        }
        let result = GameResult::Win {
            player: flagged.opponent(),
            reason: WinReason::Timeout,
        };
        self.result_override = Some(result);
        result
    }
}
