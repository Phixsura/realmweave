# Realmweave

A deterministic three-realm abstract strategy game about **connectivity,
routes, blocking, redundancy, and cross-realm control** — built as a
Rust-first engine for graph-native competitive strategy games.

> The world is a graph; strategy emerges from how players weave and sever
> paths through it.

Two players (Light and Dark) fight across three realms — **Heaven**,
**Mortal**, and **Underworld**. The flagship ruleset is **Trinity Y v4.1**,
built from exactly two rules:

1. **Y goal** — a realm is woven by whoever connects all three of its sides
   with one group. The Y theorem guarantees every realm is decisive and
   blocking IS building (attack-defense unity). First to two realms wins.
2. **Death** — a group with no adjacent empty node is captured; suicide and
   position repetition (ko) are illegal. Stones die; walls need eyes; whole
   groups can be hunted.

Three realms share one turn clock, so every stone is also a tempo decision
about *which war to fight*. Eyes, ladders, ko fights, sacrifices, and
comebacks must all emerge from the two rules — adding a third is forbidden
by design discipline (docs/design-trinity-y-v4.md).

Earlier rulesets (classic weave race, weave&sever, 层层编织 layers) remain
playable and versioned; see [docs/rules.md](docs/rules.md).

## Workspace

| Crate | Purpose |
|---|---|
| `realmweave-core` | Board graph, game state, moves, rules. Zero UI/network/DB dependencies. |
| `realmweave-bot` | Baseline AI opponents (2-ply eval search) for supported rulesets. |
| `realmweave-protocol` | Versioned client/server messages (serde). |
| `realmweave-server` | Authoritative online server: rooms, WebSocket, clocks, SQLite event logs. |
| `realmweave-client` | Native game client (Bevy): 3D stacked realms + 2D analysis view, hot-seat and online play. |
| `realmweave-sim` | Self-play simulation, balance statistics, board comparison, fairness analysis. |
| `realmweave-cli` | Board generation/validation, local terminal play, replay tooling. |

Boards are data, never code: `boards/*.json` holds the generated hex boards
(19–127 nodes per realm), all validated in CI; triangular trinity boards are
generated on demand. The archived Supply War prototype builds with
`--features supplywar-lab`.

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
