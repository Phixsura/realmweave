//! Realmweave native client (Bevy).
//!
//! Rendering is deliberately thin: all rules live in `realmweave-core`, all
//! session/transport logic in plain Rust modules (`session`, `net`). Bevy
//! systems read the session and emit `PlayerIntent`s — swapping the renderer
//! never touches game logic.

mod layout;
mod net;
mod replay;
mod session;
mod steam;
mod supplywar_ui;

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use layout::ViewMode;
use net::{NetEvent, NetHandle};
use realmweave_core::{
    boardgen, BoardGraph, EdgeKind, Game, GameResult, Move, NodeId, Player, Realm, WinReason,
};
use realmweave_protocol::{ClientMessage, ServerMessage};
use session::{Connection, Control, PlayerIntent, Session};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Realmweave".to_string(),
                resolution: (1280.0, 800.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(MeshPickingPlugin)
        .add_plugins(EguiPlugin)
        .add_plugins(steam::SteamPlugin)
        .add_plugins(supplywar_ui::SupplyWarPlugin)
        .init_resource::<UiState>()
        .init_resource::<ViewSettings>()
        .add_event::<IntentEvent>()
        .add_systems(Startup, (setup_camera, setup_cjk_font))
        .add_systems(
            Update,
            (
                menu_ui.run_if(not(resource_exists::<Active>)),
                (
                    net_pump,
                    auto_reconnect,
                    toggle_cut_mode,
                    handle_intents,
                    bot_turn,
                    replay_autoplay,
                    apply_replay_cursor,
                    sync_board_visuals,
                    orbit_camera,
                    game_hud,
                )
                    .chain()
                    .run_if(resource_exists::<Active>.and(resource_exists::<GameSession>)),
            ),
        )
        .run();
}

// ---------------------------------------------------------------- resources

/// Marker resource present while a game session is active.
#[derive(Resource)]
struct Active;

#[derive(Resource)]
struct GameSession(Session);

#[derive(Resource)]
struct Net(Option<NetHandle>);

/// Server address used for the current online session.
#[derive(Resource, Clone)]
struct ServerAddr(String);

/// Active replay (Observer mode).
#[derive(Resource)]
struct Replay(replay::ReplayState);

#[derive(Resource)]
struct ViewSettings {
    mode: ViewMode,
    show_legal: bool,
    show_components: bool,
    hovered: Option<NodeId>,
    /// Cut mode: clicking selects edge endpoints instead of placing.
    cut_mode: bool,
    /// First endpoint selected in cut mode.
    cut_anchor: Option<NodeId>,
}

impl Default for ViewSettings {
    fn default() -> Self {
        ViewSettings {
            mode: ViewMode::default(),
            show_legal: false,
            show_components: true,
            hovered: None,
            cut_mode: false,
            cut_anchor: None,
        }
    }
}

#[derive(Event)]
struct IntentEvent(PlayerIntent);

/// Menu inputs.
#[derive(Resource)]
struct UiState {
    board_size: usize,
    pie_rule: bool,
    ruleset: String,
    /// 0 = classic fixed board; otherwise a seeded random world.
    world_seed: u64,
    server_addr: String,
    room_code: String,
    replay_path: String,
    status: String,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            board_size: 61,
            pie_rule: false,
            ruleset: realmweave_core::WEAVE_SEVER_V2.to_string(),
            world_seed: 1,
            server_addr: "127.0.0.1:8420".to_string(),
            room_code: String::new(),
            replay_path: "demo-territory-hex61.json".to_string(),
            status: String::new(),
        }
    }
}

// --------------------------------------------------------------- components

#[derive(Component)]
struct NodeMarker(NodeId);

#[derive(Component)]
struct OrbitCamera {
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

/// Shared mesh handles: shape is the primary Light/Dark discriminator.
#[derive(Resource)]
struct Shapes {
    /// Light stones: smooth sphere.
    sphere: Handle<Mesh>,
    /// Dark stones: sharp diamond (double cone) — unmistakable silhouette.
    diamond: Handle<Mesh>,
    /// Empty nodes: small flat disc.
    dot: Handle<Mesh>,
}

/// Handles to the shared node materials.
#[derive(Resource)]
struct Palette {
    empty: Handle<StandardMaterial>,
    gate: Handle<StandardMaterial>,
    light: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    light_origin: Handle<StandardMaterial>,
    dark_origin: Handle<StandardMaterial>,
    legal: Handle<StandardMaterial>,
    /// Brighter tints for stones connected to at least one origin.
    light_woven: Handle<StandardMaterial>,
    dark_woven: Handle<StandardMaterial>,
    last_move: Handle<StandardMaterial>,
    light_territory: Handle<StandardMaterial>,
    dark_territory: Handle<StandardMaterial>,
}

/// Load a system CJK font so Chinese commentary renders (egui's default
/// fonts have no CJK glyphs). Tries several macOS locations, then Linux/
/// Windows fallbacks; harmless no-op if none exist.
fn setup_cjk_font(mut egui_ctx: EguiContexts) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "C:\\Windows\\Fonts\\msyh.ttc",
    ];
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            let ctx = egui_ctx.ctx_mut();
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("cjk".to_owned());
            }
            ctx.set_fonts(fonts);
            info!("loaded CJK font from {path}");
            return;
        }
    }
    warn!("no CJK font found; Chinese text will not render");
}

// ------------------------------------------------------------------- camera

fn setup_camera(mut commands: Commands) {
    // RW_AUTOSTART=supplywar[:demo] launches straight into Supply War
    // (":demo" turns the autopilot on) — used for hands-free demos.
    if let Ok(v) = std::env::var("RW_AUTOSTART") {
        if v.starts_with("supplywar") {
            let mut sw = supplywar_ui::SwSession::new(20260809);
            sw.autopilot = v.ends_with(":demo");
            commands.insert_resource(sw);
            commands.insert_resource(Active);
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

fn toggle_cut_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<ViewSettings>,
    session: Option<Res<GameSession>>,
) {
    let Some(session) = session else { return };
    if session.0.game.config().ruleset_id != realmweave_core::WEAVE_SEVER_V2 {
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        view.cut_mode = !view.cut_mode;
        view.cut_anchor = None;
    }
}

fn orbit_camera(
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

fn start_hotseat(
    commands: &mut Commands,
    size: usize,
    pie: bool,
    ruleset: &str,
    world_seed: u64,
    human_vs_bot: Option<Player>,
) {
    let def = if world_seed == 0 {
        boardgen::generate_standard(size).expect("standard size")
    } else {
        boardgen::generate_seeded(size, world_seed).expect("seeded board")
    };
    let board = BoardGraph::new(def).expect("valid board");
    let mut session = Session::hotseat_with_rules(board, pie, ruleset);
    if let Some(human) = human_vs_bot {
        session.control = Control::VsBot(human);
    }
    commands.insert_resource(GameSession(session));
    commands.insert_resource(Net(None));
    commands.insert_resource(Active);
}

/// Spawn node meshes + palette for the session's board. Called once the
/// session exists (idempotent via `BoardSpawned` marker on entities).
#[derive(Resource)]
struct BoardSpawned;

#[allow(clippy::too_many_arguments)]
fn sync_board_visuals(
    mut commands: Commands,
    session: Res<GameSession>,
    view: Res<ViewSettings>,
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
            light_territory: materials.add(node_material(Color::srgb(0.75, 0.7, 0.5), 0.2)),
            dark_territory: materials.add(node_material(Color::srgb(0.7, 0.3, 0.25), 0.2)),
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
        return;
    }
    let Some(palette) = palette else { return };
    let Some(shapes) = shapes_res else { return };

    // Node materials + positions reflect state & view mode every frame
    // (≤183 nodes — trivially cheap, keeps logic out of the renderer).
    let state = game.state();
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
    let supply_mode = game.config().ruleset_id.starts_with("three-realms-supply");
    let (light_terr, dark_terr): (
        std::collections::HashSet<NodeId>,
        std::collections::HashSet<NodeId>,
    ) = if supply_mode {
        (
            realmweave_core::supply_territory_nodes(board, state, Player::Light)
                .into_iter()
                .collect(),
            realmweave_core::supply_territory_nodes(board, state, Player::Dark)
                .into_iter()
                .collect(),
        )
    } else {
        Default::default()
    };

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
        transform.scale = Vec3::splat(scale);
        let handle = match (state.occupant(id), origins.get(&id)) {
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
            (None, _) if light_terr.contains(&id) => &palette.light_territory,
            (None, _) if dark_terr.contains(&id) => &palette.dark_territory,
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

fn node_material(color: Color, emissive_strength: f32) -> StandardMaterial {
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

fn handle_intents(
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
        }
    }
}

// -------------------------------------------------------------- net -> game

fn net_pump(
    mut commands: Commands,
    mut session: ResMut<GameSession>,
    mut ui: ResMut<UiState>,
    net: Res<Net>,
    server: Option<Res<ServerAddr>>,
) {
    let Some(handle) = &net.0 else { return };
    let session = &mut session.0;
    while let Ok(event) = handle.rx.try_recv() {
        match event {
            NetEvent::Connected => {}
            NetEvent::Disconnected(reason) => {
                if let Connection::Online { connected, .. } = &mut session.connection {
                    *connected = false;
                }
                ui.status = format!("disconnected: {reason}");
            }
            NetEvent::Message(msg) => match msg {
                ServerMessage::RoomCreated {
                    room_id,
                    token,
                    seat,
                } => {
                    session.control = Control::Seat(seat);
                    session.connection = Connection::Online {
                        room_id,
                        connected: true,
                        opponent_connected: false,
                        token,
                    };
                }
                ServerMessage::Joined {
                    room_id,
                    token,
                    seat,
                } => {
                    session.control = Control::Seat(seat);
                    session.connection = Connection::Online {
                        room_id,
                        connected: true,
                        opponent_connected: true,
                        token,
                    };
                }
                ServerMessage::Snapshot(snap) => {
                    // Authoritative rebuild of the local mirror.
                    let Some(server) = &server else { continue };
                    match net::fetch_board(&server.0, &snap.config.board_id)
                        .and_then(|def| BoardGraph::new(def).map_err(|e| e.to_string()))
                        .and_then(|board| {
                            Game::replay(board, snap.config.clone(), &snap.moves)
                                .map_err(|e| e.to_string())
                        }) {
                        Ok(game) => {
                            session.game = game;
                            session.control = Control::Seat(snap.seat);
                            session.clock = Some(snap.clock);
                            session.server_result = snap.result;
                            if let Connection::Online {
                                opponent_connected, ..
                            } = &mut session.connection
                            {
                                *opponent_connected = snap.opponent_connected;
                            }
                        }
                        Err(e) => ui.status = format!("snapshot error: {e}"),
                    }
                    commands.insert_resource(Active);
                }
                ServerMessage::MoveAccepted(event) => {
                    // Swap events are seat exchanges; a Snapshot follows and
                    // rebuilds — skip local application to avoid divergence.
                    if event.mv != Move::Swap {
                        session.apply_committed(event.mv);
                    }
                    session.clock = Some(event.clock);
                    session.last_error = None;
                }
                ServerMessage::MoveRejected { reason } => {
                    session.last_error = Some(reason);
                }
                ServerMessage::ClockUpdate(clock) => {
                    session.clock = Some(clock);
                }
                ServerMessage::GameEnded { result, clock } => {
                    session.server_result = Some(result);
                    session.clock = Some(clock);
                }
                ServerMessage::OpponentConnection { connected } => {
                    if let Connection::Online {
                        opponent_connected, ..
                    } = &mut session.connection
                    {
                        *opponent_connected = connected;
                    }
                }
                ServerMessage::Error { reason } => {
                    ui.status = reason;
                }
                ServerMessage::Pong => {}
            },
        }
    }
}

/// Automatic reconnect with a simple 3-second backoff while the online
/// connection is down and the game is unfinished.
fn auto_reconnect(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: Local<f32>,
    session: Res<GameSession>,
    ui: Res<UiState>,
) {
    let s = &session.0;
    let Connection::Online {
        connected: false, ..
    } = &s.connection
    else {
        *timer = 0.0;
        return;
    };
    if s.result().is_some() {
        return;
    }
    *timer += time.delta_secs();
    if *timer >= 3.0 {
        *timer = 0.0;
        try_reconnect(&mut commands, s, &ui);
    }
}

/// Bot turn driver: when control is VsBot and it's the bot's color to move,
/// compute a move (blocking is fine at this bot's speed: <1s typical) after
/// a short human-feeling delay.
fn bot_turn(time: Res<Time>, mut think: Local<f32>, mut session: ResMut<GameSession>) {
    let s = &mut session.0;
    let Control::VsBot(human) = s.control else {
        *think = 0.0;
        return;
    };
    if s.result().is_some() || s.game.to_move() == human {
        *think = 0.0;
        return;
    }
    *think += time.delta_secs();
    if *think < 0.6 {
        return; // brief pause so moves feel deliberate
    }
    *think = 0.0;
    let seed = 0xB07 ^ (s.game.state().ply as u64).wrapping_mul(2654435761);
    if let Some(mv) = realmweave_core::bot::choose_move(&s.game, seed) {
        let _ = s.game.play(mv);
    } else {
        // no candidate — pass if possible, else resign is never auto-played
        let _ = s.game.play(realmweave_core::Move::Pass);
    }
}

/// Demo auto-play: advance the replay cursor on a timer.
fn replay_autoplay(time: Res<Time>, replay: Option<ResMut<Replay>>) {
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
fn apply_replay_cursor(
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

fn menu_ui(mut commands: Commands, mut egui_ctx: EguiContexts, mut ui_state: ResMut<UiState>) {
    let ctx = egui_ctx.ctx_mut();
    // F1: launch Supply War with the demo AI at the wheel (used for
    // hands-free demos and remote acceptance runs).
    if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
        let mut sw = supplywar_ui::SwSession::new(20260809);
        sw.autopilot = true;
        commands.insert_resource(sw);
        commands.insert_resource(Active);
        return;
    }
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading(egui::RichText::new("REALMWEAVE").size(42.0));
            ui.label("weave paths across Heaven, Mortal, and Underworld");
            ui.add_space(30.0);
        });
        let panel_width = 420.0;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(panel_width);
            ui.heading("Board");
            ui.horizontal(|ui| {
                for size in [19usize, 37, 61, 91, 127] {
                    ui.selectable_value(&mut ui_state.board_size, size, format!("{size} × 3"));
                }
            });
            ui.checkbox(&mut ui_state.pie_rule, "pie rule (second player may swap)");
            ui.horizontal(|ui| {
                ui.label("rules");
                for (id, label) in [
                    (realmweave_core::WEAVE_SEVER_V2, "weave&sever"),
                    (realmweave_core::THREE_REALMS_V1, "classic"),
                    (realmweave_core::SEVER_V1, "sever"),
                    (realmweave_core::SUPPLY_V1, "supply"),
                    (realmweave_core::SUPPLY_RANGE_V1, "supply-range"),
                    (realmweave_core::TERRITORY_V1, "territory"),
                ] {
                    ui.selectable_value(&mut ui_state.ruleset, id.to_string(), label);
                }
            });
            ui.add_space(12.0);

            ui.heading("Supply War (prototype)");
            if ui
                .button("⚡ 开始 Supply War")
                .on_hover_text("供应线塔防灰盒原型：铺线、防御切割者、封闭裂隙")
                .clicked()
            {
                commands.insert_resource(supplywar_ui::SwSession::new(20260809));
                commands.insert_resource(Active);
            }
            ui.add_space(12.0);

            ui.heading("Local");
            ui.horizontal(|ui| {
                ui.label("world seed (0=classic)");
                let mut seed_str = ui_state.world_seed.to_string();
                if ui.text_edit_singleline(&mut seed_str).changed() {
                    if let Ok(v) = seed_str.parse::<u64>() {
                        ui_state.world_seed = v;
                    }
                }
                if ui.button("🎲").clicked() {
                    // deterministic-ish scramble from the previous value
                    ui_state.world_seed = ui_state
                        .world_seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407)
                        % 100000;
                }
            });
            if ui_state.world_seed != 0 {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "实验：种子世界会挖孔变形，目前偏向绞杀速胜",
                );
            }
            ui.horizontal(|ui| {
                if ui.button("双人热座").clicked() {
                    start_hotseat(
                        &mut commands,
                        ui_state.board_size,
                        ui_state.pie_rule,
                        &ui_state.ruleset.clone(),
                        ui_state.world_seed,
                        None,
                    );
                }
                if realmweave_core::bot::supports(&ui_state.ruleset) {
                    if ui.button("人机对战 (你执白先手)").clicked() {
                        start_hotseat(
                            &mut commands,
                            ui_state.board_size,
                            ui_state.pie_rule,
                            &ui_state.ruleset.clone(),
                            ui_state.world_seed,
                            Some(Player::Light),
                        );
                    }
                    if ui.button("人机 (你执黑后手)").clicked() {
                        start_hotseat(
                            &mut commands,
                            ui_state.board_size,
                            ui_state.pie_rule,
                            &ui_state.ruleset.clone(),
                            ui_state.world_seed,
                            Some(Player::Dark),
                        );
                    }
                } else {
                    ui.label("(此规则暂无 AI)");
                }
            });
            ui.small(
                "玩法：连接你的三个起源=编织胜 · Tab 切剪线模式(✂×3) · 永久隔离对方起源=绞杀胜",
            );
            ui.add_space(12.0);

            ui.heading("Online");
            ui.horizontal(|ui| {
                ui.label("server");
                ui.text_edit_singleline(&mut ui_state.server_addr);
            });
            if ui.button("Create private room").clicked() {
                start_online_create(&mut commands, &mut ui_state);
            }
            ui.horizontal(|ui| {
                ui.label("room code");
                ui.text_edit_singleline(&mut ui_state.room_code);
                if ui.button("Join").clicked() {
                    start_online_join(&mut commands, &mut ui_state);
                }
            });
            ui.add_space(12.0);
            ui.heading("Replay");
            ui.horizontal(|ui| {
                ui.label("record file");
                ui.text_edit_singleline(&mut ui_state.replay_path);
                if ui.button("Open").clicked() {
                    start_replay(&mut commands, &mut ui_state, 0.0);
                }
                if ui.button("Demo (30s/move)").clicked() {
                    start_replay(&mut commands, &mut ui_state, 30.0);
                }
            });
            if !ui_state.status.is_empty() {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::LIGHT_RED, &ui_state.status);
            }
        });
    });
}

fn start_online_create(commands: &mut Commands, ui: &mut UiState) {
    let handle = net::connect(&ui.server_addr);
    let board_id = format!("hex{}-v1", ui.board_size);
    // Local mirror starts from the same generated board; the server snapshot
    // will confirm.
    match net::fetch_board(&ui.server_addr, &board_id)
        .and_then(|def| BoardGraph::new(def).map_err(|e| e.to_string()))
    {
        Ok(board) => {
            let config = realmweave_core::GameConfig::new(board_id)
                .with_pie_rule(ui.pie_rule)
                .with_ruleset(&ui.ruleset)
                .with_time_control(realmweave_core::TimeControl::QUICK);
            handle.send(ClientMessage::CreateRoom {
                config: config.clone(),
            });
            let mut session = Session::hotseat(board, ui.pie_rule);
            session.control = Control::Seat(Player::Light);
            commands.insert_resource(GameSession(session));
            commands.insert_resource(Net(Some(handle)));
            commands.insert_resource(ServerAddr(ui.server_addr.clone()));
            commands.insert_resource(Active);
            ui.status.clear();
        }
        Err(e) => {
            ui.status = format!("cannot reach server: {e}");
        }
    }
}

fn start_replay(commands: &mut Commands, ui: &mut UiState, auto_seconds: f32) {
    match replay::ReplayState::load(&ui.replay_path) {
        Ok(mut state) => {
            state.auto_seconds = auto_seconds;
            state.auto_timer = auto_seconds;
            let initial = state.game_at_cursor();
            match initial {
                Ok(game) => {
                    let board = BoardGraph::new(game.board().definition().clone()).expect("board");
                    let mut session = Session::hotseat(board, false);
                    session.control = Control::Observer;
                    session.game = game;
                    commands.insert_resource(GameSession(session));
                    commands.insert_resource(Net(None));
                    commands.insert_resource(Replay(state));
                    commands.insert_resource(Active);
                    ui.status.clear();
                }
                Err(e) => ui.status = e,
            }
        }
        Err(e) => ui.status = e,
    }
}

fn start_online_join(commands: &mut Commands, ui: &mut UiState) {
    let code = ui.room_code.trim().to_uppercase();
    if code.is_empty() {
        ui.status = "enter a room code".to_string();
        return;
    }
    let handle = net::connect(&ui.server_addr);
    handle.send(ClientMessage::JoinRoom {
        room_id: code.clone(),
    });
    // Placeholder session on the default board; the server Snapshot rebuilds
    // it with the room's real config.
    let board = BoardGraph::new(boardgen::generate_standard(37).unwrap()).unwrap();
    let mut session = Session::hotseat(board, false);
    session.control = Control::Seat(Player::Dark);
    commands.insert_resource(GameSession(session));
    commands.insert_resource(Net(Some(handle)));
    commands.insert_resource(ServerAddr(ui.server_addr.clone()));
    commands.insert_resource(Active);
    ui.status.clear();
}

#[allow(clippy::too_many_arguments)]
fn game_hud(
    mut commands: Commands,
    mut egui_ctx: EguiContexts,
    session: Res<GameSession>,
    mut view: ResMut<ViewSettings>,
    mut events: EventWriter<IntentEvent>,
    mut ui_state: ResMut<UiState>,
    net: Res<Net>,
    mut replay: Option<ResMut<Replay>>,
    nodes: Query<Entity, With<NodeMarker>>,
) {
    let ctx = egui_ctx.ctx_mut();
    let s = &session.0;
    egui::TopBottomPanel::top("hud").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Realmweave").strong());
            ui.separator();
            ui.label(format!("board {}", s.game.board().definition().id));
            ui.separator();

            match s.result() {
                Some(GameResult::Win { player, reason }) => {
                    let reason = match reason {
                        WinReason::RealmWeave => "realm weave",
                        WinReason::Strangle => "strangle",
                        WinReason::Territory => "territory",
                        WinReason::Resignation => "resignation",
                        WinReason::Timeout => "timeout",
                    };
                    ui.colored_label(
                        egui::Color32::GOLD,
                        format!("{} wins by {reason}", player.name()),
                    );
                }
                Some(GameResult::Draw) => {
                    ui.colored_label(egui::Color32::GOLD, "draw");
                }
                None => {
                    ui.label(format!("{} to move", s.game.to_move().name()));
                    if let Some(pending) = s.game.state().pending_weave {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!("{} has a provisional weave!", pending.name()),
                        );
                    }
                }
            }
            ui.separator();

            if let Some(clock) = &s.clock {
                ui.label(format!(
                    "Light {}  Dark {}",
                    fmt_clock(clock.light_ms),
                    fmt_clock(clock.dark_ms)
                ));
                ui.separator();
            }

            let view_label = match view.mode {
                ViewMode::Stacked3D => "switch to 2D analysis",
                ViewMode::Analysis2D => "switch to 3D realms",
            };
            if ui.button(view_label).clicked() {
                view.mode = view.mode.toggle();
            }
            ui.checkbox(&mut view.show_legal, "legal moves");
            ui.checkbox(&mut view.show_components, "weave highlight");
            if s.game.config().ruleset_id == realmweave_core::WEAVE_SEVER_V2 {
                let label = if view.cut_mode {
                    "✂ 剪线模式 (点两个端点)"
                } else {
                    "落子模式"
                };
                if ui
                    .selectable_label(view.cut_mode, label)
                    .on_hover_text("Tab 切换：剪线模式下点击一条边的两个端点")
                    .clicked()
                {
                    view.cut_mode = !view.cut_mode;
                    view.cut_anchor = None;
                }
            }

            let observing = matches!(s.control, Control::Observer);
            if !observing {
                if s.swap_available() && ui.button("swap sides (pie)").clicked() {
                    events.send(IntentEvent(PlayerIntent::Swap));
                }
                if s.result().is_none()
                    && s.game.legal_moves().contains(&Move::Pass)
                    && ui.button("pass").clicked()
                {
                    events.send(IntentEvent(PlayerIntent::Pass));
                }
                if s.result().is_none() && ui.button("resign").clicked() {
                    events.send(IntentEvent(PlayerIntent::Resign));
                }
            }
            let charges = s.game.state().sever_charges;
            if charges != [0, 0] {
                ui.label(format!("severs L:{} D:{}", charges[0], charges[1]));
            }
            if s.game.config().ruleset_id == realmweave_core::WEAVE_SEVER_V2 {
                let sc = s.game.state().scissors;
                ui.label(egui::RichText::new(format!("✂ 白 {} | 黑 {}", sc[0], sc[1])).strong());
                // Lifelines: potential origin groups (1 = healthy).
                let lg = realmweave_core::rules::potential_origin_groups(
                    s.game.board(),
                    s.game.state(),
                    Player::Light,
                );
                let dg = realmweave_core::rules::potential_origin_groups(
                    s.game.board(),
                    s.game.state(),
                    Player::Dark,
                );
                let text = format!("生命线 白:{} 黑:{}", lifeline(lg), lifeline(dg));
                ui.label(text);
                // Strangle danger: >1 potential group means part of your
                // origins can never reconnect — permanent damage.
                if s.result().is_none() {
                    if lg > 1 {
                        ui.colored_label(egui::Color32::LIGHT_RED, "⚠ 白方起源已被割裂！");
                    }
                    if dg > 1 {
                        ui.colored_label(egui::Color32::LIGHT_RED, "⚠ 黑方起源已被割裂！");
                    }
                }
            }
            if let Some(text) = s.last_move_text() {
                ui.separator();
                ui.label(egui::RichText::new(text).italics());
            }
            if ui.button("leave").clicked() {
                commands.remove_resource::<Active>();
                commands.remove_resource::<GameSession>();
                commands.remove_resource::<Net>();
                commands.remove_resource::<BoardSpawned>();
                commands.remove_resource::<Palette>();
                commands.remove_resource::<Replay>();
                for entity in &nodes {
                    commands.entity(entity).despawn();
                }
                ui_state.status.clear();
            }
        });
        // Second row: connection status / errors.
        ui.horizontal(|ui| {
            match &s.connection {
                Connection::Local => match s.control {
                    Control::VsBot(human) => {
                        ui.label(format!("人机对战 — 你执{}", human.name()));
                        if s.result().is_none() && s.game.to_move() != human {
                            ui.colored_label(egui::Color32::YELLOW, "AI 思考中…");
                        }
                    }
                    _ => {
                        ui.label("local hot-seat");
                    }
                },
                Connection::Online {
                    room_id,
                    connected,
                    opponent_connected,
                    ..
                } => {
                    ui.label(format!("room {room_id}"));
                    if let Control::Seat(p) = s.control {
                        ui.label(format!("you play {}", p.name()));
                    }
                    if !connected {
                        ui.colored_label(egui::Color32::LIGHT_RED, "connection lost");
                        if ui.button("reconnect").clicked() {
                            try_reconnect(&mut commands, s, &ui_state);
                        }
                    } else if !opponent_connected {
                        ui.colored_label(egui::Color32::YELLOW, "waiting for opponent…");
                    }
                }
            }
            if let Some(err) = &s.last_error {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }
            if s.game
                .config()
                .ruleset_id
                .starts_with("three-realms-supply")
            {
                let l =
                    realmweave_core::supply_score(s.game.board(), s.game.state(), Player::Light);
                let d = realmweave_core::supply_score(s.game.board(), s.game.state(), Player::Dark);
                ui.separator();
                ui.label(format!(
                    "score L {} : D {}  |  captures L:{} D:{}",
                    l.display(),
                    d.display(),
                    s.game.state().captures[0],
                    s.game.state().captures[1]
                ));
            }
            if let Some(hovered) = view.hovered {
                ui.separator();
                ui.label(node_tooltip(s, hovered));
            }
            let _ = &net;
        });
        // Replay transport bar.
        if let Some(replay) = replay.as_mut() {
            // Commentary for the current move (demo/teaching mode).
            if let Some(note) = replay.0.current_annotation() {
                let heading = format!("第 {} 手 — {}", note.ply, note.player);
                let body = note.text.clone();
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(heading)
                            .strong()
                            .color(egui::Color32::GOLD),
                    );
                    ui.label(body);
                });
            }
            ui.horizontal(|ui| {
                ui.label("replay");
                // Auto-play toggle + countdown.
                if replay.0.auto_seconds > 0.0 {
                    if ui.button("⏸ pause").clicked() {
                        replay.0.auto_seconds = 0.0;
                    } else {
                        ui.label(format!(
                            "auto: next in {:.0}s",
                            replay.0.auto_timer.max(0.0)
                        ));
                    }
                } else if ui.button("▶ auto 30s").clicked() {
                    replay.0.auto_seconds = 30.0;
                    replay.0.auto_timer = 0.5;
                }
                let len = replay.0.len();
                let mut cursor = replay.0.cursor;
                if ui.button("|<").clicked() {
                    cursor = 0;
                }
                if ui.button("<").clicked() {
                    cursor = cursor.saturating_sub(1);
                }
                if ui.button(">").clicked() {
                    cursor = (cursor + 1).min(len);
                }
                if ui.button(">|").clicked() {
                    cursor = len;
                }
                let mut cursor_f = cursor as f32;
                ui.add(egui::Slider::new(&mut cursor_f, 0.0..=len as f32).integer());
                cursor = cursor_f as usize;
                if cursor != replay.0.cursor {
                    replay.0.seek(cursor);
                }
                ui.label(format!("{cursor}/{len}"));
                if let Some(result) = &replay.0.record.result {
                    ui.label(format!("final: {result:?}"));
                }
            });
        }
    });
}

/// Human-readable description of a node for the hover tooltip.
fn node_tooltip(session: &Session, id: NodeId) -> String {
    let board = session.game.board();
    let def = board.definition();
    let node = &def.nodes[id as usize];
    let realm = node.realm.name();
    let coord = node
        .axial
        .map(|ax| format!(" ({},{})", ax[0], ax[1]))
        .unwrap_or_default();
    let occupant = match session.game.state().occupant(id) {
        Some(p) => format!(" — {}", p.name()),
        None => String::new(),
    };
    let origin = def
        .origins
        .iter()
        .find(|o| o.node == id)
        .map(|o| format!(" [{} origin]", o.player.name()))
        .unwrap_or_default();
    let gate = if def.gate_nodes().contains(&id) {
        " [gate]"
    } else {
        ""
    };
    format!("#{id} {realm}{coord}{occupant}{origin}{gate}")
}

fn try_reconnect(commands: &mut Commands, session: &Session, ui: &UiState) {
    if let Connection::Online { room_id, token, .. } = &session.connection {
        let handle = net::connect(&ui.server_addr);
        handle.send(ClientMessage::Reconnect {
            room_id: room_id.clone(),
            token: token.clone(),
        });
        commands.insert_resource(Net(Some(handle)));
    }
}

fn lifeline(groups: u32) -> &'static str {
    match groups {
        1 => "完好",
        2 => "危!",
        _ => "绞杀",
    }
}

fn fmt_clock(ms: u64) -> String {
    let total = ms / 1000;
    format!("{}:{:02}", total / 60, total % 60)
}
