# Realmweave — Rules

Two players, **Light** and **Dark**; Light moves first. Boards are data
files, validated in CI for structural symmetry and fairness. The engine
ships several versioned rulesets; games record their ruleset id, so every
recorded game replays under exactly the rules it was played with.

---

## Triforce 织心 (`triforce-v5`) — the flagship

One great triangle (side 22 standard, 253 nodes; side 10 for the
tutorial). Its three corner sub-triangles are the realms — Heaven,
Mortal, Underworld — and the central inverted triangle is the
**weave-heart** where all three meet. The whole rulebook:

1. **Place.** On your turn, put a stone on any empty node. (Pie rule
   optional and recommended: Dark's first response may swap sides.)
2. **Death.** A group with no adjacent empty node (no liberties) is
   captured and removed. Your placement captures the enemy first; a move
   that leaves your own group libertyless is illegal (suicide), and
   recreating a previous whole-board position is illegal (ko).
3. **Weave.** Connect all three sides of the great triangle with one
   group — first weave wins.

That is the entire ruleset. Everything else — eyes, ladders, ko fights,
sacrifices, walls, hunts — emerges.

**Why no draws:** the Y theorem (Schensted 1953) guarantees that a full
triangle has exactly one player connecting all three sides. Blocking your
opponent's weave is building your own — attack IS defense.

**Why the realms matter:** every side of the great triangle spans two
realms, so a winning weave must cross realm borders, and all strategy
converges on the contested heart (measured: 100% of winning groups span
≥2 realms and touch the heart — docs/research-triforce.md).

---

## Trinity Y (`trinity-y-v4`)

The flagship's predecessor: three SEPARATE triangular realms (side 14),
same place+death rules per realm; a realm is won by touching its three
sides, won realms seal, two realms win the match. Kept as a variant —
its realms only couple through the shared turn clock, which human play
found too abstract (the finding that led to v5).

---

## Weave Layers (`weave-layers-v3`)

Hex boards (19–127 nodes per realm, three stacked hexagonal realms joined
at gate columns; each player has three immovable **origins**, one per
realm). Confirmed weaves (connect all three origins, survive one opponent
turn) score a **layer** and **petrify**: the winning network becomes world
structure — unplaceable, uncuttable, and traversable ONLY by the opponent
(fossil roads, open only while their owner isn't behind on layers).
Origin-adjacent stones are removed instead so origins keep breathing room.
Scissors (cut an edge permanently; 3 to start, +2 per layer, cap 4) shape
the terrain; portal edges and origin halos are protected. First to 3
layers wins; strangling (origins permanently unconnectable through
permanent terrain) wins instantly; 500-ply cap scores by layers.

## Weave & Sever (`weave-sever-v2`)

The single-weave ancestor of v3: first confirmed weave wins, scissors cut
(3 each, no resupply), strangle wins instantly, origin sanctum radius 1.

## Classic (`three-realms-v1`) and Sever (`three-realms-sever-v1`)

The original weave race: connect your three origins through your own
network and survive one opponent turn. The sever variant adds 3 charges
of enemy-stone removal each. Kept as historical baselines.

---

## Clocks

Server-enforced chess clocks (base + increment): Quick ~30min, Standard
~90min, Grand ~3h presets. Flagging loses.
