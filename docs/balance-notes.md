# Balance notes — 2026-08-09

## Round 5: TerritoryBot — a bot that plays for the actual scoreboard

New `territory` bot (`realmweave-sim --bot territory`): multi-factor static
evaluator (area diff, influence map, group-safety/liberties, weave-link
progress) with a 2-ply opponent-best-reply check on the top 8 candidates.
~0.5 s/move on hex61.

**Strength**: crushes greedy 12–0 across both seatings (6 games as Light,
6 as Dark, seeds 11/23). Games end early by resignation-scale collapses:
greedy overextends, TerritoryBot cuts supply and executes multi-stone
captures (up to 32 stones in one move in the demo game).

**Style** (metrics from a recorded hex61 self-play game):

| | greedy | territory |
|---|---|---|
| mean dist to own nearest stone | 1.36 | 1.14–1.22 |
| adjacent-to-own move % | 70–74% | 63–79% |
| captures per game | ~6 | **59** |
| game trace | parallel racing | border wars, group kills, 30+-stone massacres |

Demo artifacts: `demo-territory-hex61.json` + `.notes.json` (307 moves,
annotated) — the game turns on move 145, a 32-stone kill worth 52 points of
territory swing.

---

## Round 4: play *texture* — the "straight lines" critique

User feedback: games look like straight-line races; Go's step-by-step,
mutually-supporting feel ("步步为营") is missing. Diagnosis, two layers:

1. **Bot artifact (dominant)**: the greedy bot's only objective is
   shortest-path between origins — it draws lines by construction. Its games
   are demos of the evaluator, not of the ruleset's potential. MCTS play is
   qualitatively different but too slow for style studies at depth.
2. **Structural**: hex-graph connectivity is cheap (6 neighbors, many
   alternative paths), so loose formations carry little risk and tight
   incremental play is under-rewarded relative to Go.

Structural response: **`three-realms-supplyrange-v1`** — supply lines may
cross at most **4 empty nodes**. Own stones extend range for free, so
advancing requires *linked steps*; a stone flung deep into enemy territory
starves instantly (illegal as suicide). Cutting a chain now kills its head
even with open space beyond — local walls have Go-like killing power.

Measured (greedy, hex61, seed 7 — style metrics on one recorded game):

| Metric | unlimited supply | supply-range 4 |
|---|---|---|
| mean dist to own nearest stone | 1.36 | 1.36 |
| adjacent-to-own move % | 70-74% | 66-76% |
| balance (30 games) | 53/47 | 50/50 |

Style metrics barely move **because the greedy bot already plays adjacent
stones** — confirming diagnosis #1: the "straight lines" the user sees are
the bot's pathing, not rule-permitted looseness. The range rule's value is
*defensive semantics* (cut = kill, verified by tests), which bots this weak
cannot exploit. Next step for texture: human playtesting and/or a stronger
bot with territory-aware evaluation.

Engine facts (tested): distant placement = suicide under range rules;
chains extend range stone-by-stone; a cut chain head starves under range 4
while surviving under unlimited supply.

---

## Round 3: the supply ruleset — reaching Go-depth game length

Requirement: **300+ meaningful moves per game.** Structural diagnosis from
round 2: the weave family lacks *attack-defense unity* (blocking the
opponent costs tempo instead of building your own position), so optimal
play degenerates into a short race. The fix is `three-realms-supply-v1`,
which translates Go's liberties/captures into this game's native language:

- Every group needs a **supply line** — a path through own stones + empty
  nodes to one of its origins. Groups cut off from supply are **captured**
  (removed as a whole). Origins are never captured.
- Suicide is illegal; **positional superko** forbids repeating positions.
- Game ends on two consecutive passes (or a generous ply cap); **area
  scoring**: stones + empty regions bordered only by you + weave bonus (10)
  + **komi 2.5 to Dark** (replaces the pie rule, eliminates draws).

Attack-defense unity restored: squeezing the opponent's supply corridors
*is* expanding your own territory — no move is pure defense.

Results (greedy self-play, seed 7):

| Board (points) | games | light/dark | mean moves | fill |
|---|---|---|---|---|
| hex61 (183) | 30 | 43 / 57 % | **199** | 112%* |
| hex91 (273) | 20 | 45 / 55 % | **306** | 115%* |
| hex127 (381) | 10 | 50 / 50 % | **440** | 117%* |

\* fill >100% = capture-and-replay cycles — stones die and the land gets
re-fought over, exactly the Go-like churn we wanted.

**306 mean moves on hex91, 440 on hex127 — the 300+ target is met.** No
draws. New standard candidate: **supply rules on hex91**; hex127 for grand
matches (~3h with clocks).

### Komi calibration (vs Go's benchmark)

Go reference: black (first player) wins ~55-60% with no komi; a century of
professional play settled on komi 6.5 (Japan/Korea, black ~48-49%) / 7.5
(China, black ~47-48%); AI puts fair komi near 7.

Our sweep on hex91 (greedy, 180 games per point across seeds 7/42/2026):

| komi to Dark | Light win rate |
|---|---|
| 0.5 | **52.2%** |
| 1.5 | 45.6% |

Supply's structural first-move advantage is far smaller than Go's (a whole
extra move in Go ≈ 7 points; here the graph is small-diameter with three
symmetric fronts, so tempo dissipates). **Komi 0.5 adopted** — its main job
is eliminating draws; 52.2% ± ~4% (n=180, greedy) is within noise of fair.
Per-seed variance is large (43-67%), so treat greedy numbers as coarse;
re-calibrate with MCTS and human play before locking competitive komi.

Boards `hex91-v1` / `hex127-v1` added; suicide/superko enforced in
`legal_moves` so UI highlighting stays truthful; 8 new engine tests.

---

## Rule-variant experiments (round 2: fixing the 13-move problem)

Motivation: classic v1 games are too short and shallow — greedy self-play
ends at ~13 moves (hex19) / ~20 moves (hex37) with only 19–26% board fill.
One unblocked race decides the game. Four variants were implemented behind
the `RuleSet` abstraction and simulated:

| Ruleset (hex37, greedy × 200, seed 7) | light/dark/draw | mean moves | fill |
|---|---|---|---|
| `three-realms-v1` (classic) | 58 / 42 / 0 % | 19.7 | 19% |
| `three-realms-doubleweave-v1` | 4.5 / 5.5 / **90** % | 104 | 99% |
| `three-realms-sever-v1` (3 charges) | 58 / 42 / 0 % | 23.3 | 22% |
| `three-realms-territory-v1` | 56.5 / 35 / 8.5 % | 105 | 100% |

With MCTS (hex19, 400 playouts × 60):

| Ruleset | light/dark/draw | mean moves | fill |
|---|---|---|---|
| doubleweave | 0 / 0 / **100** % | 51 | 100% |
| sever | 45 / 33 / 22 % | 35.8 | 70% |
| territory (200 playouts × 30) | 57 / 40 / 3 % | 44.7 | 88% |

**Findings**

- **Doubleweave kills the game**: demanding 2 vertex-disjoint routes makes
  winning so hard that games peter into draws (90–100%). Rejected as-is;
  might work as a *tiebreak* or with a bigger board + fewer gates.
- **Sever is the best fix in the weave family**: games lengthen (13→36 moves
  at MCTS level), draws drop vs classic-MCTS (36%→22%), the confirmation
  turn finally has teeth (a provisional weave can actually be severed), and
  balance stays acceptable (45/33). The blitz problem shrinks because a
  single thin route is no longer safe — you must build redundancy *or* hold
  charges in reserve, which is exactly the intended design theme.
- **Territory produces full, long games** (~105 moves ≈ full board, few
  draws) and completely changes the game's nature into an area-control game
  with a weave bonus. Viable as a separate mode, not a patch to the weave
  mode.

**Recommendation**: make `three-realms-sever-v1` the default competitive
ruleset candidate; keep classic as "blitz"; keep territory as an alternate
mode; shelve doubleweave. Re-test with human games.

Repro:

```sh
target/release/realmweave-sim selfplay --board boards/hex37-v1.json \
  --games 200 --bot greedy --seed 7 --ruleset three-realms-sever-v1
target/release/realmweave-sim selfplay --board boards/hex19-v1.json \
  --games 60 --bot mcts --playouts 400 --seed 11 --ruleset three-realms-doubleweave-v1
```

---

# Balance notes (round 1) — 2026-08-09

Method: `realmweave-sim selfplay` / `compare`, seeded and deterministic per
run. Greedy bot = 0/1-BFS connection heuristic + 10% exploration noise;
MCTS = UCT with uniform random rollouts.

## First-move advantage

| Setup | first-person win rate |
|---|---|
| hex37, greedy × 500, no pie | **58.0%** |
| hex37, greedy × 500, pie (swap when est. Light WR > 50%) | **45.2%** (346 swaps) |
| hex19, mcts(300 playouts) × 100, no pie | 34.0% L / 30.0% D / 36% draw |

Findings:

- Greedy self-play shows a clear ~58% first-player advantage on hex37.
- The pie rule works as intended: with a naive rollout-based swap decision it
  slightly over-corrects (45.2%), which is what you want while openings are
  unexplored — the swap threat disciplines Light's first move.
- Stronger (MCTS) play on the small board produces long defensive games and a
  36% draw rate: at this level, breaking a weave attempt is easier than
  completing one on 19×3. Draw-rate at higher strength needs watching; if it
  persists on 37×3 with more playouts, consider rule variants (e.g. scoring
  tiebreak or weave-threat count).

## Portal topology (hex37, greedy × 300, seed 9)

| Board | gates | light WR | portal usage |
|---|---|---|---|
| hex37-v1 (inner6+outer6) | 12 | 56.7% | 45.1% |
| hex37-inner6-v1 | 6 | **67.3%** | 54.4% |

Fewer gates significantly amplify first-move advantage (67.3% vs 56.7%):
with only 6 central gate columns, the tempo of grabbing gate access first
compounds. The 12-gate layout's outer ring gives the second player viable
counter-routes. **Keep inner6+outer6 as the standard.** Structural fairness
metrics (distances, vertex cuts) are identical for both players on both
boards — the imbalance is purely tempo, which validates measuring balance by
simulation rather than static graph analysis alone.

## Suggested defaults

- `pie_rule: true` for competitive play.
- Standard board: `hex37-v1` (12 gates).
- Revisit with stronger bots (more playouts / better rollout policy) before
  locking rules for release.

Repro:

```sh
cargo build --release -p realmweave-sim
target/release/realmweave-sim selfplay --board boards/hex37-v1.json --games 500 --bot greedy --seed 7 [--pie]
target/release/realmweave-sim selfplay --board boards/hex19-v1.json --games 100 --bot mcts --playouts 300 --seed 11
cargo run -p realmweave-cli -- gen-board --size 37 --portals inner6 -o /tmp/hex37-inner6.json
target/release/realmweave-sim compare --board-a boards/hex37-v1.json --board-b /tmp/hex37-inner6.json --games 300 --bot greedy --seed 9
```
