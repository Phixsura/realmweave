//! Realmweave native client (Bevy).
//!
//! Rendering is deliberately thin: all rules live in `realmweave-core`, all
//! session/transport logic in plain Rust modules (`session`, `net`). Bevy
//! systems read the session and emit `PlayerIntent`s — swapping the renderer
//! never touches game logic.

mod board_view;
mod bots_ui;
mod hud;
mod layout;
mod menu;
mod net;
mod netsync;
mod replay;
mod replay_ui;
mod session;
mod settings;
mod steam;
#[cfg(feature = "supplywar-lab")]
mod supplywar_ui;
mod tutorial;

use board_view::*;
use bots_ui::*;
use hud::*;
use menu::*;
use netsync::*;
use replay_ui::*;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use layout::ViewMode;
use net::NetHandle;
use realmweave_core::{NodeId, Player};
use session::{Connection, Control, PlayerIntent, Session};

#[cfg(feature = "supplywar-lab")]
fn maybe_supplywar_plugin() -> supplywar_ui::SupplyWarPlugin {
    supplywar_ui::SupplyWarPlugin
}

#[cfg(not(feature = "supplywar-lab"))]
fn maybe_supplywar_plugin() -> impl Plugin {
    // No-op plugin when the lab is compiled out.
    |_: &mut App| {}
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Realmweave".to_string(),
                resolution: (1280u32, 800u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(MeshPickingPlugin)
        .add_plugins({
            // Single-pass: our egui systems live in the one Update chain
            // (interleaved with game systems); multipass would force them
            // into EguiPrimaryContextPass and split that chain. The field
            // is deprecated upstream — revisit when bevy_egui removes it.
            #[allow(deprecated)]
            EguiPlugin {
                enable_multipass_for_primary_context: false,
                ..default()
            }
        })
        .add_plugins(steam::SteamPlugin)
        .add_plugins(maybe_supplywar_plugin())
        .init_resource::<UiState>()
        .init_resource::<AiBudget>()
        .init_resource::<LocalClocks>()
        .init_resource::<ViewSettings>()
        .init_resource::<HudHeight>()
        .init_resource::<DragGuard>()
        .add_message::<IntentEvent>()
        .add_systems(Startup, (setup_camera, setup_cjk_font))
        .add_systems(
            Update,
            (
                menu_ui.run_if(not(resource_exists::<Active>)),
                (
                    net_pump,
                    auto_reconnect,
                    toggle_cut_mode,
                    shortcuts,
                    handle_intents,
                    bot_turn,
                    duel_turn,
                    replay_autoplay,
                    apply_replay_cursor,
                    sync_board_visuals,
                    orbit_camera,
                    tick_local_clocks,
                    game_hud,
                    game_over_panel,
                    history_panel,
                    tutorial_panel.run_if(resource_exists::<Tutorial>),
                    duel_panel.run_if(resource_exists::<Duel>),
                )
                    .chain()
                    .run_if(resource_exists::<Active>.and_then(resource_exists::<GameSession>)),
            ),
        )
        .run();
}

/// Accumulate thinking time for the side to move (local live games only).
fn tick_local_clocks(
    time: Res<Time>,
    mut clocks: ResMut<LocalClocks>,
    session: Option<Res<GameSession>>,
) {
    let Some(session) = session else { return };
    let s = &session.0;
    let ply = s.game.state().ply;
    if ply < clocks.last_ply {
        *clocks = LocalClocks::default(); // new game (rematch/undo past zero)
    }
    clocks.last_ply = ply;
    if s.result().is_some() || !matches!(s.connection, session::Connection::Local) {
        return;
    }
    match s.game.to_move() {
        Player::Light => clocks.light_s += time.delta_secs(),
        Player::Dark => clocks.dark_s += time.delta_secs(),
    }
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

/// Active tutorial (wraps a vs-bot session with a step panel).
#[derive(Resource)]
struct Tutorial(tutorial::TutorialState);

/// AI-vs-AI exhibition: slow-paced bot duel with live commentary.
#[derive(Resource)]
struct Duel {
    /// Seconds between moves ("slow enough to read the board").
    pace: f32,
    timer: f32,
    game_no: u32,
    games_target: u32,
    /// Rolling commentary, newest last (kept short).
    commentary: Vec<String>,
    /// Base seed; per-game variation comes from game_no.
    seed: u64,
    /// The upcoming move follows a key moment: linger longer.
    next_is_key: bool,
    /// Layer score after the previous move (to detect scoring).
    last_layers: [u8; 2],
    /// Capture totals after the previous move (to narrate deaths).
    last_captures: [u32; 2],
    /// Whether each side's "two sides touched" moment was announced.
    two_sides_announced: [bool; 2],
    /// Board settings to restart the next game with.
    board_size: usize,
    ruleset: String,
}

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
    /// Move-history side panel visibility (H toggles).
    show_history: bool,
    /// Review cursor: Some(k) = board renders the position after move k.
    review_cursor: Option<usize>,
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
            show_history: false,
            review_cursor: None,
        }
    }
}

#[derive(Message)]
struct IntentEvent(PlayerIntent);

/// Measured height of the top HUD bar this frame; side-panel roots start
/// below it (per-system egui roots don't reserve space from each other).
#[derive(Resource, Default)]
struct HudHeight(f32);

/// Cumulative left-button drag distance (px) since the button went down.
/// Bevy picking fires Click on press+release over the same entity even
/// after a long camera-orbit drag — node observers ignore clicks that
/// were really drags.
#[derive(Resource, Default)]
struct DragGuard(f32);

/// Local per-side elapsed thinking time (seconds), for the HUD.
#[derive(Resource, Default)]
struct LocalClocks {
    light_s: f32,
    dark_s: f32,
    last_ply: u32,
}

/// Active game's AI playout budget (set from the menu at game start).
#[derive(Resource, Clone, Copy)]
struct AiBudget(u32);

impl Default for AiBudget {
    fn default() -> Self {
        AiBudget(3000)
    }
}

/// AI strength presets (MCTS playout budgets).
#[derive(Clone, Copy, PartialEq, Eq)]
enum AiLevel {
    /// ~800 playouts: quick, beatable.
    Casual,
    /// ~3000 playouts: the default.
    Standard,
    /// ~8000 playouts: slow and mean.
    Strong,
}

impl AiLevel {
    fn playouts(self) -> u32 {
        match self {
            AiLevel::Casual => 800,
            AiLevel::Standard => 3000,
            AiLevel::Strong => 8000,
        }
    }
    fn label(self) -> &'static str {
        match self {
            AiLevel::Casual => "轻松",
            AiLevel::Standard => "标准",
            AiLevel::Strong => "困难",
        }
    }
}

/// Menu inputs.
#[derive(Resource)]
struct UiState {
    board_size: usize,
    pie_rule: bool,
    ruleset: String,
    /// MCTS playout budget for vs-AI games.
    ai_level: AiLevel,
    /// Triforce only: use the pierced (v5p) board — six heart nodes
    /// removed to dilute center dominance. Session-local, not persisted.
    pierced_heart: bool,
    /// 0 = classic fixed board; otherwise a seeded random world.
    world_seed: u64,
    server_addr: String,
    room_code: String,
    replay_path: String,
    status: String,
}

impl Default for UiState {
    fn default() -> Self {
        if let Some(p) = settings::load() {
            // Prefs may predate ruleset renames/deletions: unknown ids fall
            // back to the flagship instead of panicking at game start.
            let ruleset = if realmweave_core::ALL_RULESETS.contains(&p.ruleset.as_str()) {
                p.ruleset
            } else {
                realmweave_core::TRIFORCE_V5.to_string()
            };
            return UiState {
                board_size: p.board_size,
                pie_rule: p.pie_rule,
                ruleset,
                ai_level: AiLevel::Standard,
                pierced_heart: false,
                world_seed: 0,
                server_addr: p.server_addr,
                room_code: String::new(),
                replay_path: String::new(),
                status: String::new(),
            };
        }
        UiState {
            board_size: 91,
            pie_rule: false,
            ai_level: AiLevel::Standard,
            ruleset: realmweave_core::TRIFORCE_V5.to_string(),
            pierced_heart: false,
            world_seed: 0,
            server_addr: "127.0.0.1:8420".to_string(),
            room_code: String::new(),
            replay_path: String::new(),
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
    /// Region-tinted empties for merged-field boards (triforce): the three
    /// realm corners get faint identity colors; the weave-heart glows.
    empty_heaven: Handle<StandardMaterial>,
    empty_mortal: Handle<StandardMaterial>,
    empty_underworld: Handle<StandardMaterial>,
    empty_heart: Handle<StandardMaterial>,
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
    /// Petrified world structure (weave-layers-v3), tinted by which
    /// player's weave fossilized there.
    petrified_light: Handle<StandardMaterial>,
    petrified_dark: Handle<StandardMaterial>,
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
            let Ok(ctx) = egui_ctx.ctx_mut() else { return };
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
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

/// Spawn node meshes + palette for the session's board. Called once the
/// session exists (idempotent via `BoardSpawned` marker on entities).
#[derive(Resource)]
struct BoardSpawned;
