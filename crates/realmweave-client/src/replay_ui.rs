//! Split from main.rs in the world-class refactor; systems only —
//! shared resources/types stay in `main.rs` (crate root).

use bevy::prelude::*;

#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use realmweave_core::Move;

/// Demo auto-play: advance the replay cursor on a timer.
pub(crate) fn replay_autoplay(time: Res<Time>, replay: Option<ResMut<Replay>>) {
    let Some(mut replay) = replay else { return };
    if replay.0.auto_seconds <= 0.0 || replay.0.cursor >= replay.0.len() {
        return;
    }
    replay.0.auto_timer -= time.delta_secs();
    if replay.0.auto_timer <= 0.0 {
        let next = replay.0.cursor + 1;
        replay.0.seek(next);
        let per_move = replay.0.auto_seconds;
        replay.0.auto_timer = per_move;
    }
}

/// Rebuild the observed game whenever the replay cursor moved.
pub(crate) fn apply_replay_cursor(
    replay: Option<ResMut<Replay>>,
    mut session: ResMut<GameSession>,
    mut ui: ResMut<UiState>,
    mut last_cursor: Local<Option<usize>>,
) {
    let Some(replay) = replay else {
        *last_cursor = None;
        return;
    };
    if *last_cursor == Some(replay.0.cursor) {
        return;
    }
    match replay.0.game_at_cursor() {
        Ok(game) => {
            session.0.game = game;
            *last_cursor = Some(replay.0.cursor);
        }
        Err(e) => ui.status = format!("replay error: {e}"),
    }
}

// ----------------------------------------------------------------------- UI
