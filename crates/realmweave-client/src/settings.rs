//! Menu preference persistence: a small JSON file in the user's config
//! directory. Failure to read/write is never fatal — defaults win.

use serde::{Deserialize, Serialize};

/// Persisted menu preferences.
#[derive(Serialize, Deserialize)]
pub struct Prefs {
    /// Board size selection (hex rulesets).
    pub board_size: usize,
    /// Pie rule toggle.
    pub pie_rule: bool,
    /// Selected ruleset id.
    pub ruleset: String,
    /// Server address for online play.
    pub server_addr: String,
}

fn path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))?;
    Some(base.join("realmweave").join("prefs.json"))
}

/// Load preferences, if any exist.
pub fn load() -> Option<Prefs> {
    let text = std::fs::read_to_string(path()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// Persist preferences (best-effort).
pub fn save(prefs: &Prefs) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(prefs) {
        let _ = std::fs::write(p, json);
    }
}
