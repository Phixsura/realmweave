//! Steam integration skeleton, behind the `steam` feature flag.
//!
//! Scope for now: initialize the Steamworks client at startup, pump its
//! callbacks each frame, expose the local persona name, and mark extension
//! points for achievements/invites. Rooms/matchmaking stay on our own server
//! (Steam is distribution + identity, not game authority).
//!
//! Requirements at runtime (Steam builds only):
//! - Steam client running and logged in.
//! - `steam_appid.txt` next to the binary during development (480 = Spacewar
//!   test app until Realmweave has its own app id).

#[cfg(feature = "steam")]
mod enabled {
    use bevy::prelude::*;
    use steamworks::{Client, SingleClient};

    /// Non-send because the Steamworks single-threaded client is not `Sync`.
    /// `client`/`persona` are kept for the post-MVP extension points below.
    #[allow(dead_code)]
    pub struct SteamContext {
        pub client: Client,
        pub single: SingleClient,
        pub persona: String,
    }

    pub struct SteamPlugin;

    impl Plugin for SteamPlugin {
        fn build(&self, app: &mut App) {
            match Client::init() {
                Ok((client, single)) => {
                    let persona = client.friends().name();
                    info!("Steam initialized as {persona}");
                    app.insert_non_send_resource(SteamContext {
                        client,
                        single,
                        persona,
                    });
                    app.add_systems(Update, pump_callbacks);
                }
                Err(e) => {
                    // Not fatal: the game runs without Steam (dev builds,
                    // DRM-free copies).
                    warn!("Steam unavailable: {e}");
                }
            }
        }
    }

    fn pump_callbacks(steam: Option<NonSend<SteamContext>>) {
        if let Some(steam) = steam {
            steam.single.run_callbacks();
        }
    }

    // Extension points (post-MVP):
    // - achievements: steam.client.user_stats().achievement("FIRST_WEAVE").set()
    // - rich presence: steam.client.friends().set_rich_presence("status", ...)
    // - invites/join-game: friends().activate_game_overlay_invite_dialog(...)
}

#[cfg(feature = "steam")]
pub use enabled::SteamPlugin;

/// No-op plugin when built without Steam.
#[cfg(not(feature = "steam"))]
pub struct SteamPlugin;

#[cfg(not(feature = "steam"))]
impl bevy::prelude::Plugin for SteamPlugin {
    fn build(&self, _app: &mut bevy::prelude::App) {}
}
