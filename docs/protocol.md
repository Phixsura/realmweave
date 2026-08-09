# Network protocol (v1)

Transport: WebSocket at `ws://{server}/ws`, JSON text frames. All messages
are wrapped in a versioned envelope:

```json
{ "v": 1, "seq": 12, "msg": { "type": "play_move", "node": 42 } }
```

- `v` — protocol version; mismatches are rejected.
- `seq` — client→server: strictly increasing per-connection command counter
  (duplicates/stale values rejected). Server→client: outbound frame counter;
  committed room events additionally carry their own canonical `seq` inside
  `MoveAccepted`.

## Client → server (`ClientMessage`)

| type | fields | notes |
|---|---|---|
| `create_room` | `config: GameConfig` | creates a private room, seats you as Light |
| `join_room` | `room_id` | seats you as Dark; game starts when both seated; spectators rejected |
| `reconnect` | `room_id`, `token` | resume a seat after disconnect |
| `play_move` | `node` | place a stone |
| `swap_sides` | — | pie rule; server exchanges seats |
| `resign` | — | |
| `ping` | — | |

`GameConfig`: `{ ruleset_id, board_id, pie_rule, time_control? }` with
`time_control: { base_ms, increment_ms }`.

## Server → client (`ServerMessage`)

| type | fields | notes |
|---|---|---|
| `room_created` | `room_id`, `token`, `seat` | keep `token` for reconnects |
| `joined` | `room_id`, `token`, `seat` | |
| `snapshot` | `GameSnapshot` | full state: config + move log + clocks + your seat; sent on start/reconnect/seat swap |
| `move_accepted` | `MoveEvent { seq, ply, player, mv, clock }` | canonical committed move, broadcast to both seats |
| `move_rejected` | `reason` | your intent was invalid |
| `clock_update` | `ClockState` | periodic while a clocked game runs |
| `game_ended` | `result`, `clock` | weave / resignation / timeout / draw |
| `opponent_connection` | `connected` | opponent dropped or returned |
| `error` | `reason` | protocol/room errors |
| `pong` | — | |

`ClockState`: `{ light_ms, dark_ms, running }`.

## Flow

```
A: create_room ──► room_created(code, token, Light)
B: join_room(code) ──► joined(token, Dark)
both ◄── snapshot (started: true)
turn loop:
  mover: play_move ──► both ◄── move_accepted (+ clock)
  invalid ──► mover ◄── move_rejected
end: both ◄── game_ended
reconnect: reconnect(room, token) ──► snapshot; opponent ◄── opponent_connection
```

Clients rebuild state locally from `snapshot.config` + `snapshot.moves` via
`realmweave_core::Game::replay` — the engine is the single source of rule
truth on both sides, but only the server's instance is authoritative.

## HTTP endpoints

- `GET /healthz` — liveness.
- `GET /api/boards/{id}` — board definition JSON.
- `GET /api/games/{id}/record` — replayable `GameRecord` for a persisted game.
