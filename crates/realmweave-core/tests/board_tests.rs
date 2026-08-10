//! Board generation, graph construction, and validation tests.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test/tooling code
use realmweave_core::board::{BoardGraph, Edge, EdgeKind, Node, Origin};
use realmweave_core::boardgen::{self, HexBoardSpec, PortalSpec};
use realmweave_core::validate::{degree_histogram, validate_board, ValidationError};
use realmweave_core::{BoardDefinition, Player, Realm};

fn standard(size: usize) -> BoardDefinition {
    boardgen::generate_standard(size).expect("standard size")
}

#[test]
fn generates_all_standard_sizes() {
    for (size, total) in [(19, 57), (37, 111), (61, 183)] {
        let def = standard(size);
        assert_eq!(def.node_count(), total, "size {size}");
        assert_eq!(def.origins.len(), 6);
    }
}

#[test]
fn standard_boards_pass_full_validation() {
    for size in [19, 37, 61] {
        validate_board(&standard(size)).unwrap_or_else(|e| panic!("size {size}: {e}"));
    }
}

#[test]
fn realms_are_equal_size_and_realm_major() {
    let def = standard(37);
    for realm in Realm::ALL {
        let count = def.nodes.iter().filter(|n| n.realm == realm).count();
        assert_eq!(count, 37);
    }
    // Realm-major id blocks.
    for node in &def.nodes {
        let block = node.id as usize / 37;
        assert_eq!(Realm::from_index(block), Some(node.realm));
    }
}

#[test]
fn portal_counts_match_spec() {
    // 37 board: 6 inner (ring 1) + 6 outer corners (ring 2) = 12 gate columns,
    // each with 2 portal edges (H-M, M-U).
    let def = standard(37);
    let portals = def
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Portal)
        .count();
    assert_eq!(portals, 24);
    assert_eq!(def.gate_nodes().len(), 36); // 12 columns × 3 realms

    // 19 board: radius 2 → inner ring-1 gates coincide with "outer" ring-1
    // corners → 6 columns.
    let def19 = standard(19);
    let portals19 = def19
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Portal)
        .count();
    assert_eq!(portals19, 12);
}

#[test]
fn portals_link_adjacent_realms_only() {
    let def = standard(61);
    let graph = BoardGraph::new(def).unwrap();
    for edge in &graph.definition().edges {
        if edge.kind == EdgeKind::Portal {
            assert!(graph.realm_of(edge.a).is_adjacent(graph.realm_of(edge.b)));
        }
    }
}

#[test]
fn origins_are_one_per_realm_per_player() {
    let def = standard(37);
    let graph = BoardGraph::new(def).unwrap();
    for player in [Player::Light, Player::Dark] {
        let origins = graph.definition().origins_of(player);
        assert_eq!(origins.len(), 3);
        let mut realms: Vec<Realm> = origins.iter().map(|&n| graph.realm_of(n)).collect();
        realms.sort_by_key(|r| r.index());
        assert_eq!(realms, [Realm::Heaven, Realm::Mortal, Realm::Underworld]);
    }
}

#[test]
fn degree_distribution_is_hexlike() {
    let def = standard(37);
    let graph = validate_board(&def).unwrap();
    let hist = degree_histogram(&graph);
    // Hex interior degree 6; every degree present must be within 2..=8
    // (corner=3, edge=4, +up to 2 portal links).
    for (deg, &count) in &hist {
        assert!((3..=8).contains(deg), "degree {deg} x {count}");
    }
}

#[test]
fn explicit_portal_spec_is_configurable() {
    // 6-gate-only variant of the 37 board: alternative topology via data.
    let spec = HexBoardSpec {
        radius: 3,
        portals: PortalSpec::Explicit(boardgen::HEX_DIRS.iter().map(|d| [d[0], d[1]]).collect()),
    };
    let def = boardgen::generate(&spec, "hex37-inner6-x", 1);
    validate_board(&def).expect("6-gate variant validates");
    let portals = def
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Portal)
        .count();
    assert_eq!(portals, 12);
}

// --- validator rejection tests ---

fn tiny_valid_board() -> BoardDefinition {
    // Radius-1 hex (7 nodes/realm) is below standard sizes but structurally
    // valid; handy for corruption tests.
    let spec = HexBoardSpec {
        radius: 1,
        portals: PortalSpec::Explicit(vec![[0, 0]]),
    };
    // Origins are the generator's ring-1 direction corners.
    boardgen::generate(&spec, "tiny-test", 1)
}

#[test]
fn tiny_board_is_valid() {
    validate_board(&tiny_valid_board()).unwrap();
}

#[test]
fn rejects_duplicate_edge() {
    let mut def = tiny_valid_board();
    let e = def.edges[0];
    def.edges.push(Edge {
        a: e.b,
        b: e.a,
        kind: e.kind,
    });
    assert!(matches!(
        validate_board(&def),
        Err(ValidationError::DuplicateEdge(_, _))
    ));
}

#[test]
fn rejects_self_edge() {
    let mut def = tiny_valid_board();
    def.edges.push(Edge {
        a: 0,
        b: 0,
        kind: EdgeKind::IntraRealm,
    });
    assert!(matches!(
        validate_board(&def),
        Err(ValidationError::SelfEdge(0))
    ));
}

#[test]
fn rejects_isolated_node() {
    let mut def = tiny_valid_board();
    let next_id = def.nodes.len() as u16;
    def.nodes.push(Node {
        id: next_id,
        realm: Realm::Mortal,
        position: [99.0, 0.0, 99.0],
        axial: None,
    });
    assert!(matches!(
        validate_board(&def),
        Err(ValidationError::IsolatedNode(_))
    ));
}

#[test]
fn rejects_invalid_portal() {
    let mut def = tiny_valid_board();
    // Heaven ↔ Underworld direct portal: not adjacent realms.
    let heaven = def
        .nodes
        .iter()
        .find(|n| n.realm == Realm::Heaven)
        .unwrap()
        .id;
    let under = def
        .nodes
        .iter()
        .find(|n| n.realm == Realm::Underworld && n.id != heaven)
        .unwrap()
        .id;
    def.edges.push(Edge {
        a: heaven,
        b: under,
        kind: EdgeKind::Portal,
    });
    assert!(matches!(
        validate_board(&def),
        Err(ValidationError::InvalidPortal(_, _))
    ));
}

#[test]
fn rejects_shared_origin() {
    let mut def = tiny_valid_board();
    let light_origin = def
        .origins
        .iter()
        .find(|o| o.player == Player::Light)
        .unwrap()
        .node;
    if let Some(o) = def.origins.iter_mut().find(|o| o.player == Player::Dark) {
        o.node = light_origin;
    }
    assert!(matches!(
        validate_board(&def),
        Err(ValidationError::SharedOrigin(_))
    ));
}

#[test]
fn rejects_bad_origin_count() {
    let mut def = tiny_valid_board();
    def.origins.push(Origin {
        player: Player::Light,
        node: 1,
    });
    assert!(matches!(
        validate_board(&def),
        Err(ValidationError::BadOriginCount { .. })
    ));
}

#[test]
fn rejects_disconnected_graph() {
    let mut def = tiny_valid_board();
    // Remove all portals → three disconnected realm layers.
    def.edges.retain(|e| e.kind != EdgeKind::Portal);
    assert!(matches!(
        validate_board(&def),
        Err(ValidationError::Disconnected { .. })
    ));
}

#[test]
fn rejects_broken_realm_equivalence() {
    let mut def = tiny_valid_board();
    // Delete one intra-realm edge from Heaven only.
    let graph = BoardGraph::new(def.clone()).unwrap();
    let pos = def
        .edges
        .iter()
        .position(|e| e.kind == EdgeKind::IntraRealm && graph.realm_of(e.a) == Realm::Heaven)
        .unwrap();
    def.edges.remove(pos);
    let err = validate_board(&def).unwrap_err();
    assert!(
        matches!(
            err,
            ValidationError::RealmsNotEquivalent | ValidationError::NotAutomorphism(_)
        ),
        "got {err:?}"
    );
}

#[test]
fn rejects_unfair_origins() {
    let mut def = standard(37);
    // Move one Dark origin off its symmetric corner to a center-adjacent node
    // in the same realm (Mortal center ring-1). This preserves counts but
    // breaks the distance profile.
    let graph = BoardGraph::new(def.clone()).unwrap();
    let index = graph.axial_index();
    let target = index[&(Realm::Mortal, [1, 0])];
    for o in &mut def.origins {
        if o.player == Player::Dark && graph.realm_of(o.node) == Realm::Mortal {
            o.node = target;
        }
    }
    assert!(matches!(
        validate_board(&def),
        Err(ValidationError::Unfair(_))
    ));
}

#[test]
fn board_serde_round_trip() {
    let def = standard(37);
    let json = serde_json::to_string(&def).unwrap();
    let back: BoardDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(def.id, back.id);
    assert_eq!(def.nodes.len(), back.nodes.len());
    assert_eq!(def.edges, back.edges);
    assert_eq!(def.origins, back.origins);
    validate_board(&back).unwrap();
}
