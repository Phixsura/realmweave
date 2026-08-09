//! ASCII rendering of the three realms, side by side.
//!
//! Each realm is drawn as a hex diamond using axial coordinates. Cells show:
//! `.` empty, `L`/`D` stones, `l`/`d` origins, `*` marks gate columns.

use realmweave_core::{Game, Player, Realm};

pub fn render(game: &Game) -> String {
    let board = game.board();
    let def = board.definition();
    let state = game.state();
    let gates: std::collections::HashSet<_> = def.gate_nodes().into_iter().collect();
    let origins: std::collections::HashSet<_> = def.origins.iter().map(|o| o.node).collect();

    let radius = def
        .nodes
        .iter()
        .filter_map(|n| n.axial)
        .map(|ax| realmweave_core::boardgen::hex_distance(ax, [0, 0]))
        .max()
        .unwrap_or(0);
    let index = board.axial_index();

    let mut realm_blocks: Vec<Vec<String>> = Vec::new();
    for realm in Realm::ALL {
        let mut lines = Vec::new();
        lines.push(format!(
            "{:^width$}",
            realm.name(),
            width = (radius as usize * 4 + 5)
        ));
        for r in -radius..=radius {
            let indent = " ".repeat((r + radius) as usize);
            let mut row = String::new();
            for q in -radius..=radius {
                if let Some(&id) = index.get(&(realm, [q, r])) {
                    let cell = match state.occupant(id) {
                        Some(Player::Light) if origins.contains(&id) => 'l',
                        Some(Player::Dark) if origins.contains(&id) => 'd',
                        Some(Player::Light) => 'L',
                        Some(Player::Dark) => 'D',
                        None if gates.contains(&id) => '*',
                        None => '.',
                    };
                    row.push(cell);
                    row.push(' ');
                } else {
                    row.push_str("  ");
                }
            }
            lines.push(format!("{indent}{}", row.trim_end()));
        }
        realm_blocks.push(lines);
    }

    // Join the three realm blocks horizontally.
    let height = realm_blocks.iter().map(|b| b.len()).max().unwrap_or(0);
    let widths: Vec<usize> = realm_blocks
        .iter()
        .map(|b| b.iter().map(|l| l.len()).max().unwrap_or(0) + 4)
        .collect();
    let mut out = String::new();
    for i in 0..height {
        for (block, width) in realm_blocks.iter().zip(&widths) {
            let line = block.get(i).map(String::as_str).unwrap_or("");
            out.push_str(&format!("{line:<width$}"));
        }
        while out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
    }
    out.push_str("legend: L/D stones, l/d origins, * empty gate, . empty\n");
    if game.config().ruleset_id.starts_with("three-realms-supply") {
        let l = realmweave_core::supply_score(board, state, Player::Light);
        let d = realmweave_core::supply_score(board, state, Player::Dark);
        out.push_str(&format!(
            "score: Light {} (stones {} + territory {} + weave {}) | Dark {} (stones {} + territory {} + weave {} + komi {}.5) | captures L:{} D:{}\n",
            l.display(), l.stones, l.territory, l.weave_bonus,
            d.display(), d.stones, d.territory, d.weave_bonus, d.komi_half / 2,
            state.captures[0], state.captures[1],
        ));
    }
    if game.config().ruleset_id == realmweave_core::WEAVE_SEVER_V2 {
        out.push_str(&format!(
            "scissors: Light {} | Dark {}   cut edges: {}\n",
            state.scissors[0],
            state.scissors[1],
            state.cut_edges.len()
        ));
    }
    if let Some(pending) = state.pending_weave {
        out.push_str(&format!(
            "!! {} has a provisional Realm Weave — opponent has one turn to answer\n",
            pending.name()
        ));
    }
    out
}
