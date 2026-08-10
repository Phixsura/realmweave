//! ASCII rendering of the three realms, side by side.
//!
//! Each realm is drawn as a hex diamond using axial coordinates. Cells show:
//! `.` empty, `L`/`D` stones, `l`/`d` origins, `*` marks gate columns.

use realmweave_core::{Game, Player, Realm};

pub fn render(game: &Game) -> String {
    let board = game.board();
    let def = board.definition();
    if def.id.starts_with("tf") {
        return render_triforce(game);
    }
    if def.id.starts_with("tri") {
        return render_trinity(game);
    }
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

/// Triangle realms: coordinates are (row, col), row r holds r+1 cells.
/// Sides are the goals; `l`/`d` mark side cells held by each player.
fn render_trinity(game: &Game) -> String {
    let board = game.board();
    let def = board.definition();
    let state = game.state();
    let per_realm = def.nodes.len() / 3;
    let side = (((8 * per_realm + 1) as f64).sqrt() as usize - 1) / 2;
    let index = board.axial_index();

    let mut realm_blocks: Vec<Vec<String>> = Vec::new();
    let width = side * 2 + 4;
    for realm in Realm::ALL {
        let mut lines = vec![format!("{:^width$}", realm.name())];
        for r in 0..side {
            let mut row = String::new();
            for c in 0..=r {
                let ch = if let Some(&id) = index.get(&(realm, [r as i32, c as i32])) {
                    let on_side = realmweave_core::boardgen::trinity_sides(
                        side,
                        id % per_realm as u16 + (realm.index() * per_realm) as u16,
                    ) != 0;
                    match state.occupant(id) {
                        Some(Player::Light) => 'L',
                        Some(Player::Dark) => 'D',
                        None => {
                            if on_side {
                                '·'
                            } else {
                                '.'
                            }
                        }
                    }
                } else {
                    '?'
                };
                row.push(ch);
                row.push(' ');
            }
            lines.push(format!("{:^width$}", row.trim_end()));
        }
        realm_blocks.push(lines);
    }
    let height = realm_blocks.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = String::new();
    for i in 0..height {
        for block in &realm_blocks {
            let empty = String::new();
            let line = block.get(i).unwrap_or(&empty);
            out.push_str(&format!("{line:<width$}   "));
        }
        out.push('\n');
    }
    out
}

/// Triforce: one big triangle. `L`/`D` stones; empties show region —
/// `.` realm interior, `,` weave-heart, `·` big-triangle side cells.
fn render_triforce(game: &Game) -> String {
    let board = game.board();
    let def = board.definition();
    let state = game.state();
    let n = def.nodes.len();
    let side = (((8 * n + 1) as f64).sqrt() as usize - 1) / 2;
    let index = board.axial_index();
    let mut out = String::new();
    for r in 0..side {
        out.push_str(&" ".repeat(side - r));
        for c in 0..=r {
            // realm tags vary; look the id up across all realms
            let id = Realm::ALL
                .iter()
                .find_map(|&rm| index.get(&(rm, [r as i32, c as i32])))
                .copied();
            let ch = match id {
                Some(id) => match state.occupant(id) {
                    Some(Player::Light) => 'L',
                    Some(Player::Dark) => 'D',
                    None => {
                        if realmweave_core::boardgen::triforce_sides(side, id) != 0 {
                            '·'
                        } else if realmweave_core::boardgen::triforce_region(side, id) == 3 {
                            ','
                        } else {
                            '.'
                        }
                    }
                },
                None => '?',
            };
            out.push(ch);
            out.push(' ');
        }
        out.push('\n');
    }
    out
}
