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

## Store page checklist (pre-launch)

### Copy (draft)

**One-liner**: 两条规则，三个世界，零和局。围棋级的深度基因，全新的三线战略。

**Short description (EN)**: Realmweave is an abstract strategy game built
from exactly two rules: connect all three sides of a realm to weave it, and
groups without liberties die. Three triangular realms share one turn clock —
every stone is also a decision about which war to fight. No draws,
mathematically guaranteed. Play locally, against the engine, or online.

**Key selling points**
- 规则 5 分钟学会（内置交互教程），深度由数学定理背书（Y 定理：攻即是防，绝无和局）
- 三线战略层：单盘连接棋没有的"换战场"博弈
- MCTS AI 对手、AI 对弈解说演示、完整复盘/存续系统
- 权威服务器在线对战（私人房间 + 断线重连 + 棋钟）
- 确定性引擎：每一局都可精确回放分享

### Assets to produce

| Asset | Size | Notes |
|---|---|---|
| Capsule (main) | 616×353 | 3D 三界棋盘 + 一次提子瞬间 |
| Capsule (small) | 231×87 | logo + 三角形三联图形 |
| Header | 460×215 | |
| Screenshots ×5+ | 1920×1080 | 教程/人机中盘/终局面板/2D 分析视图/AI 演示解说 |
| Trailer | 30–60s | 演示模式录屏 + 解说字幕即可做首版 |

### Launch-blocking engineering items

- [ ] App ID registration + steam_appid.txt in packaging
- [ ] Achievements schema (first weave, first capture, first online win)
- [ ] Cloud saves (prefs.json + saved game records)
- [ ] Rich presence ("in a match, realm score 1-1")
