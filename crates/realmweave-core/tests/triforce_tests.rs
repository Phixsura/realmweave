//! Triforce v5: single-Y win, death rule, region metadata, replay.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use realmweave_core::rules::Triforce;
use realmweave_core::{boardgen, BoardGraph, Game, GameConfig, Move, Player, TRIFORCE_V5};

fn new_game(side: usize) -> Game {
    let b = BoardGraph::new(boardgen::generate_triforce(side).unwrap()).unwrap();
    let config = GameConfig::new(b.definition().id.clone()).with_ruleset(TRIFORCE_V5);
    Game::new(b, config).unwrap()
}

#[test]
fn boards_validate_and_regions_partition() {
    for side in [8usize, 14, 22] {
        let def = boardgen::generate_triforce(side).unwrap();
        realmweave_core::validate_board(&def).unwrap();
        let n = def.nodes.len();
        let mut counts = [0usize; 4];
        for id in 0..n as u16 {
            counts[boardgen::triforce_region(&def, side, id)] += 1;
        }
        assert_eq!(counts.iter().sum::<usize>(), n);
        // three realms equal-sized; heart smaller
        assert_eq!(counts[0], counts[1]);
        assert_eq!(counts[1], counts[2]);
        assert!(counts[3] > 0 && counts[3] < counts[0] + 1);
    }
}

#[test]
fn bottom_row_y_wins_outright() {
    let side = 8;
    let mut game = new_game(side);
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    // Light claims the bottom row (touches left+right+bottom); Dark fills
    // distinct interior cells (rows 1-2 hold 5 cells: enough for 7 replies
    // plus... use rows 1..3 = 2+3+4 = 9 cells).
    let mut dark_cells = Vec::new();
    for r in 1..4 {
        for c in 0..=r {
            dark_cells.push(idx(r, c));
        }
    }
    for (di, c) in (0..side).enumerate() {
        game.play(Move::Place(idx(side - 1, c))).unwrap();
        if game.result().is_some() {
            break;
        }
        game.play(Move::Place(dark_cells[di])).unwrap();
    }
    assert_eq!(
        game.result(),
        Some(realmweave_core::GameResult::Win {
            player: Player::Light,
            reason: realmweave_core::WinReason::RealmWeave
        })
    );
}

#[test]
fn capture_works_on_merged_board() {
    let side = 8;
    let mut game = new_game(side);
    let idx = |r: usize, c: usize| (r * (r + 1) / 2 + c) as u16;
    let target = idx(3, 1);
    let ring = [
        idx(3, 0),
        idx(3, 2),
        idx(2, 0),
        idx(2, 1),
        idx(4, 1),
        idx(4, 2),
    ];
    game.play(Move::Place(ring[0])).unwrap(); // L
    game.play(Move::Place(target)).unwrap(); // D
    let mut filler = side - 1; // bottom row cells for Dark
    for r in ring.iter().skip(1) {
        game.play(Move::Place(*r)).unwrap(); // L
        game.play(Move::Place(idx(7, filler))).unwrap(); // D far
        filler -= 1;
    }
    assert_eq!(game.state().occupant(target), None, "surrounded stone dies");
    assert_eq!(game.state().captures[0], 1);
}

#[test]
fn weaver_scan_matches_engine_result() {
    let side = 8;
    let mut game = new_game(side);
    let mut seed = 7u64;
    while game.result().is_none() {
        let legal: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m, Move::Place(_)))
            .collect();
        if legal.is_empty() {
            break;
        }
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        game.play(legal[(seed % legal.len() as u64) as usize])
            .unwrap();
    }
    let result_winner = match game.result() {
        Some(realmweave_core::GameResult::Win { player, .. }) => Some(player),
        _ => None,
    };
    assert_eq!(Triforce::weaver(game.board(), game.state()), result_winner);
}

#[test]
fn replay_reproduces_state() {
    let mut game = new_game(8);
    let mut seed = 0xFEEDu64;
    for _ in 0..40 {
        if game.result().is_some() {
            break;
        }
        let legal: Vec<Move> = game
            .legal_moves()
            .into_iter()
            .filter(|m| matches!(m, Move::Place(_)))
            .collect();
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        game.play(legal[(seed % legal.len() as u64) as usize])
            .unwrap();
    }
    let b = BoardGraph::new(boardgen::generate_triforce(8).unwrap()).unwrap();
    let replayed = Game::replay(b, game.config().clone(), &game.state().move_log).unwrap();
    assert_eq!(replayed.state(), game.state());
}

// ------------------------------------------------------------ pierced ---

#[test]
fn pierced_boards_validate_with_triangular_faces() {
    for side in [22usize, 26, 30, 40] {
        let def = boardgen::generate_triforce_pierced(side).unwrap();
        assert_eq!(def.id, format!("tf{side}-v5p"));
        realmweave_core::validate_board(&def).unwrap();
        let solid = boardgen::generate_triforce(side).unwrap();
        assert_eq!(def.nodes.len(), solid.nodes.len() - 6);
        // All internal faces triangular: for a planar triangulated disc,
        // T = E - V + 1 and 3T = 2E - B (B = boundary cycle length).
        let v = def.nodes.len() as i64;
        let e = def.edges.len() as i64;
        let b = 3 * (side as i64 - 1);
        assert_eq!((2 * e - b) % 3, 0, "side {side}: face equation");
        assert_eq!((2 * e - b) / 3, e - v + 1, "side {side}: non-triangle face");
    }
}

#[test]
fn pierced_rotation_is_an_automorphism() {
    // The validator checks the mirror; check a 120° rotation here so the
    // deletion orbit + fan chords are confirmed fully S3-symmetric.
    let side = 22usize;
    let def = boardgen::generate_triforce_pierced(side).unwrap();
    let s = side as i32 - 1;
    // (r, c) -> barycentric (u, v, w) = (s - r, c, r - c); rotation is the
    // cyclic shift (u, v, w) -> (w, u, v); back to (r, c) = (s - u', v').
    let rot = |r: i32, c: i32| -> [i32; 2] {
        let (u, v, w) = (s - r, c, r - c);
        let (u2, v2) = (w, u);
        let _ = v;
        [s - u2, v2]
    };
    let index: std::collections::HashMap<[i32; 2], u16> =
        def.nodes.iter().map(|n| (n.axial.unwrap(), n.id)).collect();
    let map: Vec<u16> = def
        .nodes
        .iter()
        .map(|n| {
            let [r, c] = n.axial.unwrap();
            *index.get(&rot(r, c)).expect("rotation image exists")
        })
        .collect();
    let edge_set: std::collections::HashSet<(u16, u16)> = def
        .edges
        .iter()
        .map(|e| (e.a.min(e.b), e.a.max(e.b)))
        .collect();
    for e in &def.edges {
        let (a, b) = (map[e.a as usize], map[e.b as usize]);
        assert!(
            edge_set.contains(&(a.min(b), a.max(b))),
            "rotated edge {}-{} missing",
            e.a,
            e.b
        );
    }
}

#[test]
fn pierced_region_and_sides_survive_renumbering() {
    let side = 22usize;
    let def = boardgen::generate_triforce_pierced(side).unwrap();
    assert_eq!(boardgen::tf_side_len(&def), side);
    let mut counts = [0usize; 4];
    for n in &def.nodes {
        counts[boardgen::triforce_region(&def, side, n.id)] += 1;
    }
    assert_eq!(counts.iter().sum::<usize>(), def.nodes.len());
    assert_eq!(counts[0], counts[1]);
    assert_eq!(counts[1], counts[2]);
    // heart lost exactly the 6 deleted nodes
    let solid = boardgen::generate_triforce(side).unwrap();
    let mut solid_counts = [0usize; 4];
    for n in &solid.nodes {
        solid_counts[boardgen::triforce_region(&solid, side, n.id)] += 1;
    }
    assert_eq!(counts[3] + 6, solid_counts[3]);
    // side masks: corners touch two sides, bottom row touches bottom
    let corner_top = def.nodes.iter().find(|n| n.axial == Some([0, 0])).unwrap();
    assert_eq!(boardgen::triforce_sides(&def, side, corner_top.id), 3);
    let br = def
        .nodes
        .iter()
        .find(|n| n.axial == Some([side as i32 - 1, 3]))
        .unwrap();
    assert_eq!(boardgen::triforce_sides(&def, side, br.id), 4);
}

#[test]
fn pierced_game_plays_and_y_wins() {
    let def = boardgen::generate_triforce_pierced(22).unwrap();
    let b = BoardGraph::new(def).unwrap();
    let config = GameConfig::new(b.definition().id.clone()).with_ruleset(TRIFORCE_V5);
    let mut game = Game::new(b, config).unwrap();
    // Bottom row is intact (holes are interior): claim it for a Y while
    // Dark answers in the top corner.
    let side = 22i32;
    let bottom: Vec<u16> = game
        .board()
        .definition()
        .nodes
        .iter()
        .filter(|n| n.axial.unwrap()[0] == side - 1)
        .map(|n| n.id)
        .collect();
    let dark: Vec<u16> = game
        .board()
        .definition()
        .nodes
        .iter()
        .filter(|n| n.axial.unwrap()[0] < 6)
        .map(|n| n.id)
        .collect();
    for (i, &n) in bottom.iter().enumerate() {
        game.play(Move::Place(n)).unwrap();
        if game.result().is_some() {
            break;
        }
        game.play(Move::Place(dark[i])).unwrap();
    }
    assert_eq!(
        game.result(),
        Some(realmweave_core::GameResult::Win {
            player: Player::Light,
            reason: realmweave_core::WinReason::RealmWeave
        })
    );
}

#[test]
fn pierced_resolves_and_unsupported_sides_refuse() {
    assert!(boardgen::resolve("tf22-v5p").is_some());
    assert!(boardgen::resolve("tf26-v5p").is_some());
    assert!(boardgen::generate_triforce_pierced(20).is_none()); // heart too small
    assert!(boardgen::generate_triforce_pierced(23).is_none()); // odd
    let p = boardgen::resolve("tf22-v5p").unwrap();
    let s = boardgen::resolve("tf22-v5").unwrap();
    assert_ne!(p.fingerprint(), s.fingerprint());
}
