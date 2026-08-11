//! Supply War presentation — SOC field edition.
//!
//! Renders the void/light fields and translates clicks into field Commands.
//! The sim is realmweave_supplywar::field (pure logic, Bevy-free).

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use realmweave_supplywar::field::{self, Command, FieldEvent, FieldState, LinkState, Outcome};
use realmweave_supplywar::{generate_map, MapSpec, SupplyMap};

pub struct SupplyWarPlugin;

impl Plugin for SupplyWarPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SwCommand>()
            .insert_resource(Time::<Fixed>::from_hz(field::TICKS_PER_SEC as f64))
            .add_systems(FixedUpdate, sw_tick.run_if(resource_exists::<SwSession>))
            .add_systems(
                Update,
                (sw_setup_scene, sw_sync_visuals, sw_camera, sw_hud)
                    .chain()
                    .run_if(resource_exists::<SwSession>),
            );
    }
}

#[derive(Resource)]
pub struct SwSession {
    pub map: SupplyMap,
    pub state: FieldState,
    pub log: Vec<(u64, Command)>,
    pub seed: u64,
    pub pending: Vec<Command>,
    pub paused: bool,
    pub autopilot: bool,
}

impl SwSession {
    pub fn new(seed: u64) -> Self {
        let map = generate_map(seed, &MapSpec::default());
        let state = FieldState::new(&map);
        SwSession {
            map,
            state,
            log: Vec::new(),
            seed,
            pending: Vec::new(),
            paused: false,
            autopilot: false,
        }
    }
}

#[derive(Message)]
struct SwCommand(Command);

#[derive(Resource)]
struct SwScene;

#[derive(Component)]
struct SwNode(realmweave_core::NodeId);

#[derive(Component)]
struct SwEdge;

#[derive(Resource)]
struct SwAssets {
    node_mesh: Handle<Mesh>,
    core_mesh: Handle<Mesh>,
    well_mesh: Handle<Mesh>,
    rift_mesh: Handle<Mesh>,
    edge_mesh: Handle<Mesh>,
    // per-entity materials so we can tint by field intensity
    node_mats: Vec<Handle<StandardMaterial>>,
    edge_mats: Vec<Handle<StandardMaterial>>,
    mat_core: Handle<StandardMaterial>,
}

fn node_pos(map: &SupplyMap, n: realmweave_core::NodeId) -> Vec3 {
    let ax = map.axial[n as usize];
    let q = ax[0] as f32;
    let r = ax[1] as f32;
    Vec3::new(3f32.sqrt() * (q + r / 2.0), 0.0, 1.5 * r)
}

fn edge_transform(map: &SupplyMap, edge: u32) -> Transform {
    let (a, b) = map.edge_endpoints(edge);
    let pa = node_pos(map, a);
    let pb = node_pos(map, b);
    let mid = (pa + pb) / 2.0;
    let dir = pb - pa;
    let len = dir.length();
    Transform::from_translation(mid)
        .looking_to(dir.normalize(), Vec3::Y)
        .with_scale(Vec3::new(1.0, 1.0, len))
}

// ----------------------------------------------------------------- tick ---

fn sw_tick(mut session: ResMut<SwSession>, mut events: MessageReader<SwCommand>) {
    for SwCommand(cmd) in events.read() {
        session.pending.push(*cmd);
    }
    if session.paused || session.state.outcome.is_some() {
        session.pending.clear();
        return;
    }
    if session.autopilot {
        let auto = autopilot(&session.map, &session.state);
        session.pending.extend(auto);
    }
    let cmds = std::mem::take(&mut session.pending);
    let next_tick = session.state.tick + 1;
    for c in &cmds {
        session.log.push((next_tick, *c));
    }
    let SwSession { map, state, .. } = &mut *session;
    field::tick(map, state, &cmds);
}

/// Demo AI on the field: extend toward wells, reinforce eroding links,
/// discharge dangerous pools near the network.
fn autopilot(map: &SupplyMap, s: &FieldState) -> Vec<Command> {
    let mut cmds = Vec::new();
    let mut budget = s.energy;
    let dist = map.distances(map.core);
    let mut wells: Vec<_> = map.wells.clone();
    wells.sort_by_key(|&w| dist[w as usize].unwrap_or(999));
    let mut trunk: Vec<u32> = Vec::new();
    for &w in wells.iter().take(5) {
        let mut cur = w;
        while cur != map.core {
            let d = dist[cur as usize].unwrap();
            if let Some(&(prev, edge)) = map.adjacency[cur as usize]
                .iter()
                .find(|(n, _)| dist[*n as usize] == Some(d - 1))
            {
                trunk.push(edge);
                cur = prev;
            } else {
                break;
            }
        }
    }
    trunk.sort_unstable();
    trunk.dedup();
    for &e in &trunk {
        if budget < field::COST_BUILD {
            break;
        }
        if matches!(s.links[e as usize], LinkState::Empty | LinkState::Broken) {
            let (a, b) = map.edge_endpoints(e);
            if s.on_network[a as usize] || s.on_network[b as usize] {
                cmds.push(Command::BuildLink(e));
                budget -= field::COST_BUILD;
            }
        }
    }
    if budget > field::COST_DISCHARGE + field::COST_REINFORCE {
        if let Some(&e) = trunk
            .iter()
            .find(|&&e| s.links[e as usize] == LinkState::Single && s.link_hp[e as usize] < 0.7)
        {
            cmds.push(Command::Reinforce(e));
            budget -= field::COST_REINFORCE;
        }
    }
    if budget >= field::COST_DISCHARGE {
        // discharge the deepest pool within range of the network
        let mut best: Option<(f32, u16)> = None;
        for nd in 0..map.node_count() as u16 {
            let v = s.void[nd as usize];
            if v > field::CRITICAL * 0.7 && s.refractory[nd as usize] == 0 {
                let d = map.distances(nd);
                let near = (0..map.node_count())
                    .any(|i| s.on_network[i] && d[i].unwrap_or(99) <= field::DISCHARGE_RANGE);
                if near && best.map(|(bv, _)| v > bv).unwrap_or(true) {
                    best = Some((v, nd));
                }
            }
        }
        if let Some((_, nd)) = best {
            cmds.push(Command::Discharge(nd));
        }
    }
    cmds
}

// ---------------------------------------------------------------- scene ---

fn sw_setup_scene(
    mut commands: Commands,
    session: Res<SwSession>,
    scene: Option<Res<SwScene>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if scene.is_some() {
        return;
    }
    let map = &session.map;
    let mk = |c: Color, e: f32| -> StandardMaterial {
        let l = c.to_linear();
        StandardMaterial {
            base_color: c,
            emissive: LinearRgba::new(l.red * e, l.green * e, l.blue * e, 1.0),
            perceptual_roughness: 0.55,
            ..default()
        }
    };
    let mut node_mats = Vec::with_capacity(map.node_count());
    for _ in 0..map.node_count() {
        node_mats.push(materials.add(mk(Color::srgb(0.3, 0.3, 0.38), 0.05)));
    }
    let mut edge_mats = Vec::with_capacity(map.edges.len());
    for _ in 0..map.edges.len() {
        edge_mats.push(materials.add(StandardMaterial {
            base_color: Color::srgba(0.5, 0.5, 0.7, 0.10),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }));
    }
    let assets = SwAssets {
        node_mesh: meshes.add(Sphere::new(0.20).mesh().ico(2).unwrap()),
        core_mesh: meshes.add(Sphere::new(0.55).mesh().ico(3).unwrap()),
        well_mesh: meshes.add(Mesh::from(Cone {
            radius: 0.32,
            height: 0.65,
        })),
        rift_mesh: meshes.add(Mesh::from(Cuboid::new(0.5, 0.9, 0.5))),
        edge_mesh: meshes.add(Mesh::from(Cuboid::new(0.12, 0.07, 1.0))),
        node_mats,
        edge_mats,
        mat_core: materials.add(mk(Color::srgb(1.0, 0.85, 0.3), 2.5)),
    };

    for nid in 0..map.node_count() as realmweave_core::NodeId {
        let pos = node_pos(map, nid);
        let mesh = if nid == map.core {
            assets.core_mesh.clone()
        } else if map.wells.contains(&nid) {
            assets.well_mesh.clone()
        } else if map.rifts.contains(&nid) {
            assets.rift_mesh.clone()
        } else {
            assets.node_mesh.clone()
        };
        let mat = if nid == map.core {
            assets.mat_core.clone()
        } else {
            assets.node_mats[nid as usize].clone()
        };
        let id = nid;
        commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_translation(pos),
                SwNode(nid),
            ))
            .observe(
                move |trigger: On<Pointer<Click>>, mut events: MessageWriter<SwCommand>| {
                    // Right-click (or secondary) = discharge at this node.
                    if trigger.button == PointerButton::Secondary {
                        events.write(SwCommand(Command::Discharge(id)));
                    }
                },
            );
    }
    for e in 0..map.edges.len() as u32 {
        let id = e;
        commands
            .spawn((
                Mesh3d(assets.edge_mesh.clone()),
                MeshMaterial3d(assets.edge_mats[e as usize].clone()),
                edge_transform(map, e),
                SwEdge,
            ))
            .observe(
                move |trigger: On<Pointer<Click>>,
                      mut events: MessageWriter<SwCommand>,
                      session: Res<SwSession>| {
                    if trigger.button != PointerButton::Primary {
                        return;
                    }
                    let cmd = match session.state.links[id as usize] {
                        LinkState::Empty | LinkState::Broken => Command::BuildLink(id),
                        LinkState::Single => Command::Reinforce(id),
                        _ => return,
                    };
                    events.write(SwCommand(cmd));
                },
            );
    }
    commands.insert_resource(assets);
    commands.insert_resource(SwScene);
}

// --------------------------------------------------------------- visuals --

fn sw_sync_visuals(
    session: Res<SwSession>,
    assets: Option<Res<SwAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut nodes: Query<(&SwNode, &mut Transform), Without<SwEdge>>,
    mut gizmos: Gizmos,
) {
    let Some(assets) = assets else { return };
    let map = &session.map;
    let s = &session.state;

    // Node materials: continuous field visualization.
    for (SwNode(nid), mut tf) in &mut nodes {
        let i = *nid as usize;
        if *nid == map.core {
            continue;
        }
        let mut mat = materials.get_mut(&assets.node_mats[i]).unwrap();
        let v = s.void[i];
        let l = s.light[i];
        let refr = s.refractory[i];
        let (color, emissive, scale) = if refr > 0 {
            // collapse crater: white flash fading
            let f = refr as f32 / field::REFRACTORY_TICKS as f32;
            (Color::srgb(1.0, 1.0, 1.0), 3.0 * f, 1.0 + 0.6 * f)
        } else if map.rifts.contains(nid) {
            (
                Color::srgb(0.65, 0.15, 0.85),
                1.2 + (v / 20.0).min(1.5),
                1.0,
            )
        } else if map.wells.contains(nid) {
            if s.on_network[i] {
                (Color::srgb(0.3, 1.0, 0.5), 1.8, 1.0)
            } else {
                (Color::srgb(0.2, 0.45, 0.3), 0.2, 1.0)
            }
        } else if l > 0.4 && l >= v {
            let t = (l / 6.0).min(1.0);
            (
                Color::srgb(0.85 + 0.15 * t, 0.8 + 0.15 * t, 0.55 + 0.2 * t),
                0.4 + 1.4 * t,
                1.0,
            )
        } else if v > 0.4 {
            let t = (v / field::CRITICAL).min(1.0);
            (
                Color::srgb(0.25 + 0.45 * t, 0.08, 0.3 + 0.5 * t),
                0.2 + 1.2 * t,
                1.0 + 0.35 * t,
            )
        } else {
            (Color::srgb(0.3, 0.3, 0.38), 0.05, 1.0)
        };
        mat.base_color = color;
        let lc = color.to_linear();
        mat.emissive = LinearRgba::new(
            lc.red * emissive,
            lc.green * emissive,
            lc.blue * emissive,
            1.0,
        );
        tf.scale = Vec3::splat(scale);
    }

    // Edge materials: link state + hp.
    for e in 0..map.edges.len() {
        let mut mat = materials.get_mut(&assets.edge_mats[e]).unwrap();
        match s.links[e] {
            LinkState::Empty => {
                mat.base_color = Color::srgba(0.5, 0.5, 0.7, 0.08);
                mat.emissive = LinearRgba::NONE;
                mat.alpha_mode = AlphaMode::Blend;
            }
            LinkState::Building(t) => {
                let p = 1.0 - t as f32 / field::BUILD_TICKS as f32;
                mat.base_color = Color::srgb(0.5 + 0.3 * p, 0.5 + 0.25 * p, 0.4);
                mat.emissive = LinearRgba::new(0.3 * p, 0.3 * p, 0.15 * p, 1.0);
                mat.alpha_mode = AlphaMode::Opaque;
            }
            LinkState::Single | LinkState::Reinforced => {
                let hp = s.link_hp[e];
                let reinforced = s.links[e] == LinkState::Reinforced;
                let base = if reinforced {
                    Color::srgb(0.55, 0.85, 1.0)
                } else {
                    Color::srgb(1.0, 0.9, 0.6)
                };
                // damage shows as reddening
                let dmg = 1.0 - hp;
                let c = base.to_linear();
                mat.base_color = Color::srgb(c.red + dmg * 0.3, c.green * hp, c.blue * hp);
                mat.emissive = LinearRgba::new((0.8 + dmg) * hp.max(0.3), 0.7 * hp, 0.4 * hp, 1.0);
                mat.alpha_mode = AlphaMode::Opaque;
            }
            LinkState::Broken => {
                mat.base_color = Color::srgba(0.4, 0.1, 0.1, 0.35);
                mat.emissive = LinearRgba::NONE;
                mat.alpha_mode = AlphaMode::Blend;
            }
        }
    }

    // Energy flow shimmer on live links.
    let t = s.tick as f32 / field::TICKS_PER_SEC as f32;
    for e in 0..map.edges.len() as u32 {
        if !s.is_link_alive(e) {
            continue;
        }
        let (a, b) = map.edge_endpoints(e);
        if !(s.on_network[a as usize] && s.on_network[b as usize]) {
            continue;
        }
        let pa = node_pos(map, a);
        let pb = node_pos(map, b);
        let phase = (t * 0.9 + e as f32 * 0.41).fract();
        let p = pa.lerp(pb, phase) + Vec3::Y * 0.09;
        gizmos.sphere(
            Isometry3d::from_translation(p),
            0.05,
            Color::srgb(1.0, 0.95, 0.6),
        );
    }
}

// ---------------------------------------------------------------- camera --

fn sw_camera(
    mut query: Query<&mut Transform, With<Camera3d>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    time: Res<Time>,
    mut zoom: Local<f32>,
) {
    let Ok(mut tf) = query.single_mut() else {
        return;
    };
    if *zoom == 0.0 {
        *zoom = 26.0;
    }
    for ev in wheel.read() {
        *zoom = (*zoom - ev.y * 1.5).clamp(10.0, 45.0);
    }
    let mut pan = Vec3::ZERO;
    let speed = 14.0 * time.delta_secs();
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        pan.z -= speed;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        pan.z += speed;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        pan.x -= speed;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        pan.x += speed;
    }
    let focus_x = tf.translation.x + pan.x;
    let focus_z = tf.translation.z - (*zoom) * 0.7 + pan.z + (*zoom) * 0.7;
    tf.translation = Vec3::new(focus_x, *zoom, focus_z);
    let look_at = Vec3::new(focus_x, 0.0, focus_z - (*zoom) * 0.7);
    tf.look_at(look_at, Vec3::Y);
}

// ------------------------------------------------------------------- HUD --

fn sw_hud(
    mut commands: Commands,
    mut egui_ctx: EguiContexts,
    mut session: ResMut<SwSession>,
    nodes: Query<Entity, With<SwNode>>,
    edges: Query<Entity, With<SwEdge>>,
) {
    let Ok(ctx) = egui_ctx.ctx_mut() else { return };
    let mut toggle_pause = false;
    let mut toggle_ap = false;
    let mut restart_seed: Option<u64> = None;
    // Hot path reads through a scoped shared borrow — the per-frame
    // SupplyMap/FieldState clones were pure allocation churn.
    {
        let session_ref = &*session;
        let map = &session_ref.map;
        let s = &session_ref.state;
        let paused = session_ref.paused;
        let autopilot = session_ref.autopilot;

        let mut root = crate::hud::root_ui(ctx, "sw_hud_root");
        egui::Panel::top("sw_hud").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Supply War · 虚空场").strong());
            ui.separator();
            ui.label(format!(
                "⚡ {:.0}/{:.0} (+{:.1}/s)",
                s.energy,
                field::ENERGY_CAP,
                s.income_per_sec(map)
            ));
            ui.separator();
            let remain = field::GAME_LENGTH_SECS as f32 - s.seconds();
            ui.label(format!("剩余 {:.0}s", remain.max(0.0)));
            ui.separator();
            let wells = map
                .wells
                .iter()
                .filter(|&&w| s.on_network[w as usize])
                .count();
            ui.label(format!("井 {wells}/{}", map.wells.len()));
            ui.separator();
            // core danger meter
            let core_void = s.void[map.core as usize];
            if core_void > 1.0 {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("⚠ 母核虚空 {core_void:.1}/{}", field::CORE_DROWN_LEVEL),
                );
                ui.separator();
            }
            // last avalanche
            if let Some(FieldEvent::Avalanche { size, tick }) = s
                .events
                .iter()
                .rev()
                .find(|e| matches!(e, FieldEvent::Avalanche { .. }))
            {
                ui.label(format!(
                    "上次雪崩: {size} 节点 (t={:.0}s)",
                    *tick as f32 / 10.0
                ));
                ui.separator();
            }
            if ui.button(if paused { "▶" } else { "⏸" }).clicked() {
                toggle_pause = true;
            }
            if ui
                .selectable_label(autopilot, if autopilot { "🤖 AI代打中" } else { "🤖 AI演示" })
                .clicked()
            {
                toggle_ap = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!(
                "左键点边=铺线({:.0}⚡)/加固({:.0}⚡) · 右键点紫色深渊=引爆泄压({:.0}⚡) · WASD+滚轮=视角",
                field::COST_BUILD,
                field::COST_REINFORCE,
                field::COST_DISCHARGE
            ));
        });
    });
        if let Some(outcome) = s.outcome {
            let mut root = crate::hud::root_ui(ctx, "sw_outcome_root");
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(egui::Color32::from_black_alpha(160)))
                .show(&mut root, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(120.0);
                        let (title, color) = match outcome {
                            Outcome::Victory { wells_lit } => (
                                format!("守住了黎明 —— {wells_lit} 口井仍在燃烧"),
                                egui::Color32::GOLD,
                            ),
                            Outcome::Defeat { .. } => {
                                ("母核沉入虚空".to_string(), egui::Color32::LIGHT_RED)
                            }
                        };
                        ui.label(egui::RichText::new(title).size(34.0).color(color));
                        ui.add_space(10.0);
                        let quakes: Vec<u32> = s
                            .events
                            .iter()
                            .filter_map(|e| match e {
                                FieldEvent::Avalanche { size, .. } => Some(*size),
                                _ => None,
                            })
                            .collect();
                        let biggest = quakes.iter().max().copied().unwrap_or(0);
                        ui.label(format!(
                            "历经 {:.0}s | 雪崩 {} 次，最大 {} 节点连锁",
                            s.seconds(),
                            quakes.len(),
                            biggest
                        ));
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.add_space(ui.available_width() / 2.0 - 130.0);
                            if ui.button("同种子再来").clicked() {
                                restart_seed = Some(session_ref.seed);
                            }
                            if ui.button("换个世界").clicked() {
                                restart_seed = Some(session_ref.seed.wrapping_add(1));
                            }
                        });
                    });
                });
        }
    } // end shared borrow

    if toggle_pause {
        session.paused = !session.paused;
    }
    if toggle_ap {
        session.autopilot = !session.autopilot;
    }
    if let Some(seed) = restart_seed {
        restart(&mut commands, seed, &nodes, &edges);
    }
}

fn restart(
    commands: &mut Commands,
    seed: u64,
    nodes: &Query<Entity, With<SwNode>>,
    edges: &Query<Entity, With<SwEdge>>,
) {
    for e in nodes.iter().chain(edges.iter()) {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<SwScene>();
    commands.remove_resource::<SwAssets>();
    commands.insert_resource(SwSession::new(seed));
}
