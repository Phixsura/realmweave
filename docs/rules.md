# Realmweave — Rules

Realmweave is played on three stacked realms — **Heaven**, **Mortal**,
**Underworld** — each the same sixfold-symmetric hex graph, connected only at
**gate columns** (Heaven↔Mortal, Mortal↔Underworld). Two players, **Light**
and **Dark**, each own three immovable **Origins**, one per realm, placed
graph-symmetrically.

Standard boards: 19 / 37 / 61 / **91 (standard)** / 127 (grand) nodes per
realm. Boards are data files, validated for structural symmetry and origin
fairness.

The engine ships several versioned rulesets. **Supply is the current
competitive default**; the others remain available for casual play and
experiments.

---

## Supply rules (`three-realms-supply-v1`) — default

The heart of the game: **stones live on supply lines.**

### Placement and supply

- Players alternate; Light moves first. A stone goes on any empty node.
- Every group (connected set of own stones) must have a **supply line**: a
  path through *own stones and empty nodes* leading to any of its owner's
  origins.
- After each placement, enemy groups left without supply are **captured** —
  removed from the board as a whole. Captures happen before checking your
  own supply, so a capturing move is legal even if it would otherwise seal
  your own group.
- **Origins are never captured** — they are their own supply.
- **Suicide is illegal**: a placement that leaves your own group unsupplied
  (after captures resolve) is not allowed.
- **Superko**: recreating any earlier whole-board position is illegal.

### Ending and scoring

- The game ends when both players **pass** consecutively (or the safety cap
  of 6×nodes plies is reached).
- **Area scoring**:
  - +1 per stone on the board (origins count),
  - +1 per empty node in regions bordered *only* by your stones,
  - +10 **weave bonus** if your three origins are connected through your
    network,
  - Dark receives **komi 0.5** (calibrated by simulation; also breaks ties).
- Higher score wins. Resignation and clock timeout end the game immediately.

### Why these rules

Cutting the opponent's supply corridors *is* expanding your own territory —
attack and defense are the same move, which is what gives the game its
depth. Expect Go-like phases: opening (claim corridors), middle game
(cut/counter-cut, capture races), endgame (border sealing worth single
points). Typical game length: ~200 moves (61 board), ~300 (91), ~440 (127).

---

## Classic rules (`three-realms-v1`) — blitz

- Stones never move and are never captured; enemy stones block.
- **Realm Weave**: connect your three origins through your own network. The
  weave is *provisional* for one opponent turn; if it still stands at the
  start of your next turn, you win.
- Full board with no confirmed weave: standing provisional weave wins,
  otherwise draw.
- Optional **pie rule**: after Light's first placement Dark may swap seats.
- Fast (typically under 30 moves) and tactical — the "blitz" mode.

## Sever rules (`three-realms-sever-v1`)

Classic, plus each player holds **3 sever charges**: instead of placing, you
may remove one enemy non-origin stone. A provisional weave can therefore be
physically broken during the response turn. Middle ground between classic
speed and supply depth.

## Double weave (`three-realms-doubleweave-v1`)

Classic, but a weave requires **two internally-vertex-disjoint routes**
between every origin pair. Redundancy is mandatory. Experimental: very
draw-heavy at bot level.

## Territory rules (`three-realms-territory-v1`)

No weave win; pass is legal; two passes end the game. Score = largest
connected network + 15 weave bonus. Simpler precursor to supply (no
captures); kept for comparison.

---

## Time controls

Chess-style clocks, data-driven presets:

| Preset | Base | Increment | Intended board |
|---|---|---|---|
| Quick | 12 min | 5 s | 19 / 37 |
| Standard | 40 min | 15 s | 61 / 91 |
| Grand | 70 min | 30 s | 127 |
