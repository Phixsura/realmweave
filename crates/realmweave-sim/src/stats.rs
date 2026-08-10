//! Aggregate statistics over batches of self-play games.

use realmweave_core::{Game, GameResult, Move, Player, WinReason};

#[derive(Default, Debug)]
pub struct BatchStats {
    pub games: u32,
    pub light_wins: u32,
    pub dark_wins: u32,
    pub draws: u32,
    pub weave_wins: u32,
    pub move_counts: Vec<u32>,
    pub fill_ratios: Vec<f64>,
    pub portal_usage: Vec<f64>,
    /// Ply at which the winning weave was completed (provisional).
    pub weave_plies: Vec<u32>,
    /// Wins by the person who *started* as Light (seat-swap aware; only
    /// differs from light_wins when the pie rule triggered a swap).
    pub first_person_wins: u32,
    pub swaps: u32,
}

impl BatchStats {
    /// Record a game where the person who started as Light now plays
    /// `person_a_plays` (differs from Light only after a pie-rule swap).
    pub fn record_with_persons(&mut self, game: &Game, person_a_plays: Player) {
        if person_a_plays == Player::Dark {
            self.swaps += 1;
        }
        if let Some(GameResult::Win { player, .. }) = game.result() {
            if player == person_a_plays {
                self.first_person_wins += 1;
            }
        }
        self.games += 1;
        match game.result() {
            Some(GameResult::Win { player, reason }) => {
                match player {
                    Player::Light => self.light_wins += 1,
                    Player::Dark => self.dark_wins += 1,
                }
                if reason == WinReason::RealmWeave {
                    self.weave_wins += 1;
                    self.weave_plies.push(game.state().ply);
                }
            }
            Some(GameResult::Draw) => self.draws += 1,
            None => {}
        }
        let state = game.state();
        let placements = state
            .move_log
            .iter()
            .filter(|m| matches!(m, Move::Place(_)))
            .count();
        self.move_counts.push(placements as u32);
        let capacity = game.board().node_count() - game.board().definition().origins.len();
        self.fill_ratios.push(placements as f64 / capacity as f64);

        // Portal usage: fraction of gate nodes occupied at game end.
        let gates = game.board().definition().gate_nodes();
        let used = gates
            .iter()
            .filter(|&&g| state.occupant(g).is_some())
            .count();
        self.portal_usage
            .push(used as f64 / gates.len().max(1) as f64);
    }

    pub fn summary(&self, label: &str) -> String {
        let mean = |v: &[u32]| v.iter().map(|&x| x as f64).sum::<f64>() / v.len().max(1) as f64;
        let meanf = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        let mut sorted = self.move_counts.clone();
        sorted.sort_unstable();
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0);
        format!(
            "{label}\n\
             games                {}\n\
             light win rate       {:.1}%\n\
             dark win rate        {:.1}%\n\
             draw rate            {:.1}%\n\
             weave win rate       {:.1}%\n\
             mean moves           {:.1} (median {})\n\
             mean fill ratio      {:.1}%\n\
             mean portal usage    {:.1}%\n\
             mean weave ply       {:.1}\n\
             first-person wins    {:.1}% (pie swaps: {})",
            self.games,
            100.0 * self.light_wins as f64 / self.games.max(1) as f64,
            100.0 * self.dark_wins as f64 / self.games.max(1) as f64,
            100.0 * self.draws as f64 / self.games.max(1) as f64,
            100.0 * self.weave_wins as f64 / self.games.max(1) as f64,
            mean(&self.move_counts),
            median,
            100.0 * meanf(&self.fill_ratios),
            100.0 * meanf(&self.portal_usage),
            mean(&self.weave_plies),
            100.0 * self.first_person_wins as f64 / self.games.max(1) as f64,
            self.swaps,
        )
    }
}
