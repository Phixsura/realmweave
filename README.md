# Realmweave

A deterministic three-realm abstract strategy game about **connectivity,
routes, blocking, redundancy, and cross-realm control** — built as a
Rust-first engine for graph-native competitive strategy games.

> The world is a graph; strategy emerges from how players weave and sever
> paths through it.

Two players (Light and Dark) place stones across three stacked realms —
**Heaven**, **Mortal**, and **Underworld** — connected by a limited set of
portal gates. Under the default **supply rules**, every group of stones must
keep a supply line back to its origins; groups cut off are captured, and the
game is decided by area scoring (stones + territory + weave bonus + komi).
Games run 200–440 moves depending on board size — opening, middle game, and
endgame phases emerge like in Go, but the tactics are pure connectivity
across three realms.

See [docs/rules.md](docs/rules.md) for the complete rules.

## Workspace

| Crate | Purpose |
|---|---|
| `realmweave-core` | Board graph, game state, moves, rules. Zero UI/network/DB dependencies. |
| `realmweave-protocol` | Versioned client/server messages (serde). |
| `realmweave-server` | Authoritative online server: rooms, WebSocket, clocks, SQLite event logs. |
| `realmweave-client` | Native game client (Bevy): 3D stacked realms + 2D analysis view, hot-seat and online play. |
| `realmweave-sim` | Self-play simulation, balance statistics, board comparison, fairness analysis. |
| `realmweave-cli` | Board generation/validation, local terminal play, replay tooling. |

Boards are data, never code: `boards/*.json` holds the generated
19/37/61-nodes-per-realm hex boards, all validated in CI.

## Quick start

```sh
# run all tests
cargo test --workspace

# validate the shipped boards
cargo run -p realmweave-cli -- validate boards/*.json

# play a local two-player game in the terminal (supply rules by default)
cargo run -p realmweave-cli -- play --board boards/hex91-v1.json
# blitz mode (classic weave race)
cargo run -p realmweave-cli -- play --board boards/hex37-v1.json --ruleset three-realms-v1

# launch the game client (hot-seat works without a server)
cargo run -p realmweave-client

# run the online server, then create/join rooms from two clients
cargo run -p realmweave-server -- --listen 127.0.0.1:8420 --boards boards

# balance tooling
cargo run -p realmweave-sim -- selfplay --board boards/hex37-v1.json --games 200
cargo run -p realmweave-sim -- fairness --board boards/hex37-v1.json
cargo run -p realmweave-sim -- compare --board-a boards/hex19-v1.json --board-b boards/hex37-v1.json
```

### Online play

1. Start the server.
2. Client A: **Create private room** → shares the 6-letter room code.
3. Client B: enters the code → **Join**.
4. The server owns state and clocks; clients only send intent. Disconnected
   players can reconnect with their seat token (automatic in the client UI).
5. Finished games persist as replayable event logs in SQLite; export via
   `GET /api/games/{id}/record` and step through with
   `realmweave-cli replay`.

## Documentation

- [docs/rules.md](docs/rules.md) — current Three Realms ruleset (v1)
- [docs/architecture.md](docs/architecture.md) — crate boundaries and design rules
- [docs/board-topology.md](docs/board-topology.md) — hex board family, gates, origins, fairness
- [docs/protocol.md](docs/protocol.md) — network protocol reference

## Development principles

- **One authoritative Rust rules engine, reused everywhere.**
- Rules are versioned and pluggable (`RuleSet` trait); the current victory
  rule is experimental and cheap to replace.
- Boards are validated structurally, for realm equivalence, for symmetry
  (graph automorphisms), and for origin fairness — never by eyeballing.
- Deterministic: identical config + move log always reproduces identical
  state; every finished game is replayable.
- Distribution target is a native desktop client (Steam); rendering is
  modular and never touches rules.

## License

Apache-2.0
