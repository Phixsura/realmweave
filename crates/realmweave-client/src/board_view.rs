//! Split from main.rs in the world-class refactor; systems only —
//! shared resources/types stay in `main.rs` (crate root).

use bevy::prelude::*;

#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use realmweave_core::Move;

pub(crate) fn setup_camera(mut commands: Commands) {
    // RW_AUTOSTART=supplywar[:demo] launches straight into Supply War
    // (":demo" turns the autopilot on) — needs the supplywar-lab feature.
    if let Ok(v) = std::env::var("RW_AUTOSTART") {
        if v.starts_with("supplywar") {
            #[cfg(feature = "supplywar-lab")]
            {
                let mut sw = supplywar_ui::SwSession::new(20260809);
                sw.autopilot = v.ends_with(":demo");
                commands.insert_resource(sw);
                commands.insert_resource(Active);
            }
        } else if let Some(rest) = v.strip_prefix("duel") {
            // RW_AUTOSTART=duel        → weave-layers-v3 exhibition
            // RW_AUTOSTART=duel:v4     → trinity-y-v4 exhibition
            let (ruleset, def) = if rest == ":v4" {
                (
                    realmweave_core::TRINITY_Y_V4,
                    boardgen::generate_trinity(14).expect("trinity board"),
                )
            } else {
                (
                    realmweave_core::WEAVE_LAYERS_V3,
                    boardgen::generate_standard(91).expect("standard size"),
                )
            };
            let board = BoardGraph::new(def).expect("valid board");
            let mut session = Session::hotseat_with_rules(board, false, ruleset);
            session.control = Control::BotDuel;
            commands.insert_resource(GameSession(session));
            commands.insert_resource(Net(None));
            commands.insert_resource(Active);
            commands.insert_resource(Duel {
                pace: 2.5,
                timer: 0.0,
                game_no: 1,
                games_target: 3,
                commentary: vec!["—— 第 1 局开始 ——".to_string()],
                seed: 0xD0E1,
                next_is_key: false,
                last_layers: [0, 0],
                last_captures: [0, 0],
                board_size: 91,
                ruleset: ruleset.to_string(),
            });
        }
    }
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 14.0, 22.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera {
            focus: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.55,
            distance: 26.0,
        },
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(8.0, 16.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 400.0,
    });
    commands.insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.06)));
}

pub(crate) fn orbit_camera(
    mut query: Query<(&mut OrbitCamera, &mut Transform)>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
    mut egui_ctx: EguiContexts,
) {
    let over_ui = egui_ctx.ctx_mut().wants_pointer_input();
    let Ok((mut orbit, mut transform)) = query.get_single_mut() else {
        return;
    };
    let mut rotate = Vec2::ZERO;
    let mut pan = Vec2::ZERO;
    for ev in motion.read() {
        if over_ui {
            continue;
        }
        if buttons.pressed(MouseButton::Right)
            || (buttons.pressed(MouseButton::Left) && !buttons.just_pressed(MouseButton::Left))
        {
            rotate += ev.delta;
        }
        if buttons.pressed(MouseButton::Middle) {
            pan += ev.delta;
        }
    }
    let mut zoom = 0.0;
    for ev in wheel.read() {
        if !over_ui {
            zoom += ev.y;
        }
    }
    orbit.yaw -= rotate.x * 0.008;
    orbit.pitch = (orbit.pitch + rotate.y * 0.008).clamp(-1.5, 1.5);
    orbit.distance = (orbit.distance - zoom * 1.2).clamp(6.0, 90.0);
    if pan != Vec2::ZERO {
        let right = transform.right();
        let up = transform.up();
        let scale = orbit.distance * 0.0015;
        let focus_delta = -right * pan.x * scale + up * pan.y * scale;
        orbit.focus += focus_delta;
    }
    let rot = Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0);
    let offset = rot * Vec3::new(0.0, 0.0, orbit.distance);
    transform.translation = orbit.focus + offset;
    transform.look_at(orbit.focus, Vec3::Y);
}

// ----------------------------------------------------------- session set-up

/// Keyboard shortcuts for local play: U = undo; ←/→ = step review.
pub(crate) fn shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    session: Option<Res<GameSession>>,
    mut view: ResMut<ViewSettings>,
    mut events: EventWriter<IntentEvent>,
) {
    let Some(session) = session else { return };
    let s = &session.0;
    let total = s.game.state().move_log.len();
    if keys.just_pressed(KeyCode::KeyU)
        && matches!(s.connection, Connection::Local)
        && !matches!(s.control, Control::Observer | Control::BotDuel)
        && total > 0
    {
        view.review_cursor = None;
        events.send(IntentEvent(PlayerIntent::Undo));
    }
    // Arrow keys drive the review cursor (opens the history panel too).
    if total > 0 && keys.just_pressed(KeyCode::ArrowLeft) {
        let cur = view.review_cursor.unwrap_or(total);
        view.review_cursor = Some(cur.saturating_sub(1));
        view.show_history = true;
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        if let Some(cur) = view.review_cursor {
            view.review_cursor = if cur + 1 >= total {
                None
            } else {
                Some(cur + 1)
            };
        }
    }
}

pub(crate) fn toggle_cut_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<ViewSettings>,
    session: Option<Res<GameSession>>,
) {
    let Some(session) = session else { return };
    if !matches!(
        session.0.game.config().ruleset_id.as_str(),
        realmweave_core::WEAVE_SEVER_V2 | realmweave_core::WEAVE_LAYERS_V3
    ) {
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        view.cut_mode = !view.cut_mode;
        view.cut_anchor = None;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn sync_board_visuals(
    mut commands: Commands,
    session: Res<GameSession>,
    view: Res<ViewSettings>,
    tut: Option<Res<Tutorial>>,
    time: Res<Time>,
    mut camera: Query<(&mut OrbitCamera, &mut Transform), Without<NodeMarker>>,
    mut anim: Local<(u32, f32)>, // (ply we saw last, seconds since it changed)
    spawned: Option<Res<BoardSpawned>>,
    palette: Option<Res<Palette>>,
    shapes_res: Option<Res<Shapes>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut nodes: Query<(
        &NodeMarker,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Mesh3d,
    )>,
    mut gizmos: Gizmos,
) {
    let game = &session.0.game;
    let board = game.board();

    if spawned.is_none() {
        let palette = Palette {
            empty: materials.add(node_material(Color::srgb(0.4, 0.42, 0.5), 0.05)),
            gate: materials.add(node_material(Color::srgb(0.5, 0.55, 0.9), 0.4)),
            // Light = warm ivory-gold; Dark = vivid crimson. Chosen for
            // maximum separation from each other, the background, and the
            // gray empty markers.
            light: materials.add(node_material(Color::srgb(1.0, 0.95, 0.8), 1.6)),
            dark: materials.add(node_material(Color::srgb(0.95, 0.2, 0.15), 1.3)),
            light_origin: materials.add(node_material(Color::srgb(1.0, 0.85, 0.3), 3.0)),
            dark_origin: materials.add(node_material(Color::srgb(1.0, 0.3, 0.1), 3.0)),
            legal: materials.add(node_material(Color::srgb(0.3, 0.95, 0.6), 0.9)),
            light_woven: materials.add(node_material(Color::srgb(1.0, 1.0, 0.9), 2.4)),
            dark_woven: materials.add(node_material(Color::srgb(1.0, 0.35, 0.25), 2.2)),
            last_move: materials.add(node_material(Color::srgb(0.3, 0.85, 1.0), 2.8)),
            petrified_light: materials.add(node_material(Color::srgb(0.55, 0.52, 0.42), 0.0)),
            petrified_dark: materials.add(node_material(Color::srgb(0.45, 0.36, 0.36), 0.0)),
        };
        let shapes = Shapes {
            sphere: meshes.add(Sphere::new(0.34).mesh().ico(3).unwrap()),
            diamond: meshes.add(
                Mesh::from(Cone {
                    radius: 0.34,
                    height: 0.45,
                })
                .translated_by(Vec3::Y * 0.0),
            ),
            dot: meshes.add(Sphere::new(0.10).mesh().ico(2).unwrap()),
        };
        let sphere = shapes.sphere.clone();
        for node in &board.definition().nodes {
            let pos = layout::node_position(board, node.id, view.mode);
            let id = node.id;
            commands
                .spawn((
                    Mesh3d(sphere.clone()),
                    MeshMaterial3d(palette.empty.clone()),
                    Transform::from_translation(Vec3::from_array(pos)),
                    NodeMarker(id),
                ))
                .observe(
                    move |_trigger: Trigger<Pointer<Click>>,
                          mut events: EventWriter<IntentEvent>,
                          mut view: ResMut<ViewSettings>,
                          session: Res<GameSession>| {
                        let s = &session.0;
                        if view.review_cursor.is_some() {
                            return; // review mode is read-only
                        }
                        if view.cut_mode {
                            // Two clicks pick an edge to cut.
                            match view.cut_anchor {
                                None => view.cut_anchor = Some(id),
                                Some(anchor) if anchor == id => view.cut_anchor = None,
                                Some(anchor) => {
                                    let def = s.game.board().definition();
                                    if let Some(e) = def.edges.iter().position(|e| {
                                        (e.a == anchor && e.b == id) || (e.a == id && e.b == anchor)
                                    }) {
                                        events.send(IntentEvent(PlayerIntent::CutEdge(e as u32)));
                                        view.cut_anchor = None;
                                    } else {
                                        // Not adjacent: restart from here.
                                        view.cut_anchor = Some(id);
                                    }
                                }
                            }
                            return;
                        }
                        let me = s.game.to_move();
                        let intent = match s.game.state().occupant(id) {
                            // Enemy stone + charges left → sever attempt.
                            Some(p) if p != me => PlayerIntent::SeverStone(id),
                            _ => PlayerIntent::PlaceStone(id),
                        };
                        events.send(IntentEvent(intent));
                    },
                )
                .observe(
                    move |_trigger: Trigger<Pointer<Over>>, mut view: ResMut<ViewSettings>| {
                        view.hovered = Some(id);
                    },
                )
                .observe(
                    move |_trigger: Trigger<Pointer<Out>>, mut view: ResMut<ViewSettings>| {
                        if view.hovered == Some(id) {
                            view.hovered = None;
                        }
                    },
                );
        }
        commands.insert_resource(palette);
        commands.insert_resource(shapes);
        commands.insert_resource(BoardSpawned);
        // Auto-frame the camera to the board's extent (hex-91 ≈ 11 world
        // units of radius; triangles are narrower — a fixed distance either
        // dwarfs small boards or crops large ones).
        let max_r = board
            .definition()
            .nodes
            .iter()
            .map(|n| (n.position[0].powi(2) + n.position[2].powi(2)).sqrt())
            .fold(0.0f32, f32::max);
        for (mut orbit, _) in &mut camera {
            orbit.distance = (max_r * 2.6).clamp(14.0, 40.0);
        }
        return;
    }
    let Some(palette) = palette else { return };
    let Some(shapes) = shapes_res else { return };

    // Node materials + positions reflect state & view mode every frame
    // (≤400 nodes — trivially cheap, keeps logic out of the renderer).
    // Review mode: render the historical position at the cursor instead.
    let review_state = view.review_cursor.and_then(|k| {
        let bd = BoardGraph::new(game.board().definition().clone()).ok()?;
        let moves = &game.state().move_log[..k.min(game.state().move_log.len())];
        realmweave_core::Game::replay(bd, game.config().clone(), moves)
            .ok()
            .map(|g| g.state().clone())
    });
    let state = review_state.as_ref().unwrap_or_else(|| game.state());
    let def = board.definition();
    let origins: std::collections::HashMap<NodeId, Player> =
        def.origins.iter().map(|o| (o.node, o.player)).collect();
    let gates: std::collections::HashSet<NodeId> = def.gate_nodes().into_iter().collect();
    let legal: std::collections::HashSet<NodeId> = if view.show_legal {
        session.0.legal_placements().into_iter().collect()
    } else {
        Default::default()
    };
    let woven: std::collections::HashMap<NodeId, Player> = if view.show_components {
        [Player::Light, Player::Dark]
            .into_iter()
            .flat_map(|p| {
                session
                    .0
                    .origin_connected(p)
                    .into_iter()
                    .map(move |n| (n, p))
            })
            .collect()
    } else {
        Default::default()
    };
    let last_placed = session.0.last_placed();
    // Placement pop animation: 0 → full scale over 0.25s after each move.
    let ply = game.state().ply;
    if anim.0 != ply {
        *anim = (ply, 0.0);
    } else {
        anim.1 += time.delta_secs();
    }
    let pop = (anim.1 / 0.25).min(1.0);
    // ease-out-back: overshoot slightly then settle
    let pop = 1.0 + 1.7 * (pop - 1.0).powi(3) + 0.7 * (pop - 1.0).powi(2) * (pop - 1.0).abs();
    let pop = pop.clamp(0.05, 1.15);

    for (marker, mut transform, mut material, mut mesh) in &mut nodes {
        let id = marker.0;
        let pos = layout::node_position(board, id, view.mode);
        transform.translation = Vec3::from_array(pos);
        // Shape IS the side: sphere = Light, diamond = Dark, dot = empty.
        let (target_mesh, scale) = match (state.occupant(id), origins.contains_key(&id)) {
            (Some(Player::Light), true) => (&shapes.sphere, 1.5),
            (Some(Player::Dark), true) => (&shapes.diamond, 1.5),
            (Some(Player::Light), false) => (&shapes.sphere, 1.0),
            (Some(Player::Dark), false) => (&shapes.diamond, 1.0),
            (None, _) => (&shapes.dot, 1.0),
        };
        if mesh.0 != *target_mesh {
            mesh.0 = target_mesh.clone();
        }
        let scale = if last_placed == Some(id) && view.review_cursor.is_none() {
            scale * pop
        } else {
            scale
        };
        transform.scale = Vec3::splat(scale);
        let handle = match (state.occupant(id), origins.get(&id)) {
            _ if state.petrified_by(id) == Some(Player::Light) => &palette.petrified_light,
            _ if state.petrified_by(id) == Some(Player::Dark) => &palette.petrified_dark,
            _ if last_placed == Some(id) => &palette.last_move,
            (Some(Player::Light), Some(_)) => &palette.light_origin,
            (Some(Player::Dark), Some(_)) => &palette.dark_origin,
            (Some(Player::Light), None) if woven.get(&id) == Some(&Player::Light) => {
                &palette.light_woven
            }
            (Some(Player::Dark), None) if woven.get(&id) == Some(&Player::Dark) => {
                &palette.dark_woven
            }
            (Some(Player::Light), None) => &palette.light,
            (Some(Player::Dark), None) => &palette.dark,
            (None, _) if legal.contains(&id) => &palette.legal,
            (None, _) if gates.contains(&id) => &palette.gate,
            (None, _) => &palette.empty,
        };
        if material.0 != *handle {
            material.0 = handle.clone();
        }
    }

    // Edges & portals as gizmo lines (immediate mode: works in both views).
    // Cut edges are simply GONE — the world changed. Exception: the *freshest*
    // cut is drawn as a bright red scar so the player sees what just happened.
    let cut: std::collections::HashSet<u32> = state.cut_edges.iter().copied().collect();
    let last_cut = session.0.last_cut();
    for (ei, edge) in def.edges.iter().enumerate() {
        if cut.contains(&(ei as u32)) {
            if last_cut == Some(ei as u32) {
                let a = Vec3::from_array(layout::node_position(board, edge.a, view.mode));
                let b = Vec3::from_array(layout::node_position(board, edge.b, view.mode));
                // broken line: draw the two outer thirds, gap in the middle
                let t1 = a.lerp(b, 0.38);
                let t2 = a.lerp(b, 0.62);
                let scar = Color::srgb(1.0, 0.15, 0.1);
                gizmos.line(a, t1, scar);
                gizmos.line(t2, b, scar);
                gizmos.sphere(Isometry3d::from_translation(a.lerp(b, 0.5)), 0.18, scar);
            }
            continue;
        }
        let a = Vec3::from_array(layout::node_position(board, edge.a, view.mode));
        let b = Vec3::from_array(layout::node_position(board, edge.b, view.mode));
        let color = match edge.kind {
            EdgeKind::IntraRealm => match (state.occupant(edge.a), state.occupant(edge.b)) {
                (Some(Player::Light), Some(Player::Light)) => Color::srgb(1.0, 0.95, 0.6),
                (Some(Player::Dark), Some(Player::Dark)) => Color::srgb(1.0, 0.3, 0.2),
                _ => Color::srgba(0.45, 0.48, 0.62, 0.35),
            },
            EdgeKind::Portal => match (state.occupant(edge.a), state.occupant(edge.b)) {
                (Some(Player::Light), Some(Player::Light)) => Color::srgb(1.0, 0.9, 0.4),
                (Some(Player::Dark), Some(Player::Dark)) => Color::srgb(1.0, 0.4, 0.25),
                _ => Color::srgba(0.6, 0.5, 0.9, 0.6),
            },
        };
        gizmos.line(a, b, color);
    }

    // In the analysis view, mark portal correspondence between the side-by-
    // side boards with soft arcs above the layers.
    if view.mode == ViewMode::Analysis2D {
        for edge in def.edges.iter().filter(|e| e.kind == EdgeKind::Portal) {
            let a = Vec3::from_array(layout::node_position(board, edge.a, view.mode));
            let b = Vec3::from_array(layout::node_position(board, edge.b, view.mode));
            let mid = (a + b) / 2.0 + Vec3::Y * 2.0;
            gizmos.line(a, mid, Color::srgba(0.6, 0.5, 0.9, 0.35));
            gizmos.line(mid, b, Color::srgba(0.6, 0.5, 0.9, 0.35));
        }
    }

    // Duel: show each side's cheapest origin-linking routes as faint trails
    // so the viewer can SEE what both AIs are trying to do.
    if session.0.control == Control::BotDuel {
        for (player, color) in [
            (Player::Light, Color::srgba(1.0, 0.95, 0.5, 0.28)),
            (Player::Dark, Color::srgba(1.0, 0.35, 0.25, 0.28)),
        ] {
            for n in realmweave_bot::best_routes(&session.0.game, player) {
                if state.occupant(n).is_none() {
                    let p = Vec3::from_array(layout::node_position(board, n, view.mode));
                    gizmos.sphere(Isometry3d::from_translation(p), 0.28, color);
                }
            }
        }
    }

    // Tutorial guidance: pulse suggested nodes and edges.
    if let Some(tut) = &tut {
        let hints = tut.0.hints(&session.0.game);
        let pulse = 0.45 + 0.35 * (time.elapsed_secs() * 3.0).sin();
        let glow = Color::srgba(0.3, 1.0, 0.55, pulse);
        for &n in &hints.nodes {
            let p = Vec3::from_array(layout::node_position(board, n, view.mode));
            gizmos.sphere(
                Isometry3d::from_translation(p),
                0.6 + 0.1 * (time.elapsed_secs() * 3.0).sin(),
                glow,
            );
        }
        for &e in &hints.edges {
            let edge = &def.edges[e as usize];
            let a = Vec3::from_array(layout::node_position(board, edge.a, view.mode));
            let b = Vec3::from_array(layout::node_position(board, edge.b, view.mode));
            gizmos.line(a, b, glow);
        }
    }

    // Last-move pulse: a breathing ring around the freshest stone.
    if let Some(last) = session.0.last_placed() {
        let p = Vec3::from_array(layout::node_position(board, last, view.mode));
        let r = 0.55 + 0.12 * (time.elapsed_secs() * 4.0).sin();
        gizmos.sphere(
            Isometry3d::from_translation(p),
            r,
            Color::srgba(1.0, 1.0, 1.0, 0.55),
        );
    }

    // Cut-mode anchor highlight.
    if let Some(anchor) = view.cut_anchor {
        let p = Vec3::from_array(layout::node_position(board, anchor, view.mode));
        gizmos.sphere(
            Isometry3d::from_translation(p),
            0.55,
            Color::srgb(1.0, 0.5, 0.1),
        );
    }

    // Realm layer rings for readability (3D view).
    if view.mode == ViewMode::Stacked3D {
        let max_r = def
            .nodes
            .iter()
            .map(|n| Vec2::new(n.position[0], n.position[2]).length())
            .fold(0.0f32, f32::max)
            + 0.8;
        for realm in Realm::ALL {
            let y = match realm {
                Realm::Heaven => boardgen::LAYER_HEIGHT,
                Realm::Mortal => 0.0,
                Realm::Underworld => -boardgen::LAYER_HEIGHT,
            };
            let color = match realm {
                Realm::Heaven => Color::srgba(0.9, 0.9, 1.0, 0.25),
                Realm::Mortal => Color::srgba(0.6, 0.9, 0.7, 0.25),
                Realm::Underworld => Color::srgba(0.9, 0.5, 0.5, 0.25),
            };
            let rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
            gizmos.circle(
                Isometry3d::new(Vec3::new(0.0, y, 0.0), rotation),
                max_r,
                color,
            );
        }
    }
}

pub(crate) fn node_material(color: Color, emissive_strength: f32) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: color,
        emissive: LinearRgba::new(
            linear.red * emissive_strength,
            linear.green * emissive_strength,
            linear.blue * emissive_strength,
            1.0,
        ),
        perceptual_roughness: 0.4,
        metallic: 0.1,
        ..default()
    }
}

// ------------------------------------------------------------------ intents

pub(crate) fn handle_intents(
    mut events: EventReader<IntentEvent>,
    mut session: ResMut<GameSession>,
    net: Res<Net>,
) {
    for IntentEvent(intent) in events.read() {
        let session = &mut session.0;
        match (&net.0, intent) {
            (None, intent) => {
                // Hot-seat: the engine validates every intent.
                session.apply_local(intent);
            }
            (Some(handle), PlayerIntent::PlaceStone(node)) => {
                if session.is_my_turn() {
                    handle.send(ClientMessage::PlayMove { node: *node });
                }
            }
            (Some(handle), PlayerIntent::SeverStone(node)) => {
                if session.is_my_turn() {
                    handle.send(ClientMessage::SeverStone { node: *node });
                }
            }
            (Some(handle), PlayerIntent::CutEdge(edge)) => {
                if session.is_my_turn() {
                    handle.send(ClientMessage::CutEdge { edge: *edge });
                }
            }
            (Some(handle), PlayerIntent::Pass) => {
                if session.is_my_turn() {
                    handle.send(ClientMessage::Pass);
                }
            }
            (Some(handle), PlayerIntent::Swap) => {
                handle.send(ClientMessage::SwapSides);
            }
            (Some(handle), PlayerIntent::Resign) => {
                handle.send(ClientMessage::Resign);
            }
            (Some(_), PlayerIntent::Undo) => {
                // Undo is local-only; the server is authoritative online.
            }
        }
    }
}

// -------------------------------------------------------------- net -> game
