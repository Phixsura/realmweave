# Steam distribution notes

Status: **pre-integration skeleton.** The client builds with optional
Steamworks support; no app id has been registered yet.

## Build matrix

| Build | Command | Behavior |
|---|---|---|
| Plain | `cargo build --release -p realmweave-client` | No Steam code compiled in. |
| Steam | `cargo build --release -p realmweave-client --features steam` | Initializes Steamworks at startup; **gracefully degrades** if the Steam client is unavailable (dev machines, DRM-free copies). |

Packaging: `scripts/package.sh [--steam]` → `dist/realmweave-<platform>[-steam]/`
containing the binary, `boards/`, docs, and (steam builds) `steam_appid.txt`.

## What Steam is (and is not) used for

- **Is**: distribution, identity (persona name), later achievements /
  rich presence / overlay invites. Extension points are marked in
  `crates/realmweave-client/src/steam.rs`.
- **Is not**: game authority. Rooms, clocks, and move validation stay on the
  Realmweave server; Steam networking is not used for game state.

## Before first upload (checklist)

1. Register the app on Steamworks; replace the dev app id (480/Spacewar) in
   `scripts/package.sh`.
2. Depot layout: one depot per platform from `dist/realmweave-<platform>-steam/`.
   Launch option: `realmweave-client` (no arguments; server address is
   user-configurable in the UI).
3. Ship the Steamworks redistributable (`libsteam_api` dylib/so/dll) next to
   the binary — `steamworks-rs` links it dynamically.
4. macOS: codesign + notarize the .app bundle (to be scripted once an Apple
   Developer identity exists).
5. Decide default server endpoint(s) and bake a sensible default into the
   client UI before wide distribution.
