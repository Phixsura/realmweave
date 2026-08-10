//! SQLite persistence: games + ordered event logs.
//!
//! Schema is deliberately plain SQL so it ports to PostgreSQL later. A
//! completed game is reconstructible from its config + ordered move events —
//! no per-move snapshots are stored.

use realmweave_core::{GameConfig, GameResult, Move, Player};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

/// SQLite-backed persistence for games and their ordered event logs.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Open (creating if missing) the database and run migrations.
    pub async fn open(path: &str) -> Result<Self, sqlx::Error> {
        let options: SqliteConnectOptions = path
            .parse::<SqliteConnectOptions>()?
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS games (
                id           TEXT PRIMARY KEY,
                ruleset_id   TEXT NOT NULL,
                board_id     TEXT NOT NULL,
                config_json  TEXT NOT NULL,
                created_at   INTEGER NOT NULL,
                finished_at  INTEGER,
                result_json  TEXT
            );
            CREATE TABLE IF NOT EXISTS events (
                game_id    TEXT NOT NULL REFERENCES games(id),
                seq        INTEGER NOT NULL,
                ply        INTEGER NOT NULL,
                player     TEXT NOT NULL,
                move_json  TEXT NOT NULL,
                ts         INTEGER NOT NULL,
                clock_json TEXT NOT NULL,
                PRIMARY KEY (game_id, seq)
            );
            "#,
        )
        .execute(&pool)
        .await?;
        Ok(Store { pool })
    }

    /// Insert a new game row.
    pub async fn create_game(&self, id: &str, config: &GameConfig) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO games (id, ruleset_id, board_id, config_json, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&config.ruleset_id)
        .bind(&config.board_id)
        .bind(serde_json::to_string(config).unwrap_or_default())
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    /// Append one committed move to the game's ordered event log.
    pub async fn append_event(
        &self,
        game_id: &str,
        seq: u64,
        ply: u32,
        player: Player,
        mv: &Move,
        clock_json: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO events (game_id, seq, ply, player, move_json, ts, clock_json)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(game_id)
        .bind(seq as i64)
        .bind(ply as i64)
        .bind(player.name())
        .bind(serde_json::to_string(mv).unwrap_or_default())
        .bind(now_ms())
        .bind(clock_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a game finished with its result.
    pub async fn finish_game(&self, game_id: &str, result: &GameResult) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE games SET finished_at = ?, result_json = ? WHERE id = ?")
            .bind(now_ms())
            .bind(serde_json::to_string(result).unwrap_or_default())
            .bind(game_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Reconstructible record: config + ordered move log (for replay tooling
    /// and the export endpoint).
    pub async fn load_record(
        &self,
        game_id: &str,
    ) -> Result<Option<realmweave_core::GameRecord>, sqlx::Error> {
        let Some(row) = sqlx::query("SELECT config_json, result_json FROM games WHERE id = ?")
            .bind(game_id)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };
        let config: GameConfig = serde_json::from_str(row.get::<String, _>("config_json").as_str())
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let result: Option<GameResult> = row
            .get::<Option<String>, _>("result_json")
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let rows = sqlx::query("SELECT move_json FROM events WHERE game_id = ? ORDER BY seq")
            .bind(game_id)
            .fetch_all(&self.pool)
            .await?;
        let moves = rows
            .iter()
            .map(|r| serde_json::from_str(r.get::<String, _>("move_json").as_str()))
            .collect::<Result<Vec<Move>, _>>()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok(Some(realmweave_core::GameRecord {
            config,
            moves,
            result,
        }))
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0) // pre-1970 clock: log-worthy but never fatal
}
