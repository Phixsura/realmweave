# Realmweave — Rules

Two players, **Light** and **Dark**; Light moves first. Boards are data
files, validated in CI for structural symmetry and fairness. The engine
ships several versioned rulesets; games record their ruleset id, so every
recorded game replays under exactly the rules it was played with.

---

## Trinity Y (`trinity-y-v4`) — the flagship

Played on three **triangular** realms — Heaven, Mortal, Underworld — side
length 14 standard (8 for the tutorial). The whole rulebook:

1. **Place.** On your turn, put a stone on any empty node of any open
   realm. (Pie rule optional: Dark's first response may swap sides.)
2. **Death.** A group with no adjacent empty node (no liberties) is
   captured and removed. Your placement captures the enemy first; a move
   that leaves your own group libertyless is illegal (suicide), and
   recreating a previous whole-board position is illegal (ko).
3. **Weave.** A realm is woven by the player whose single connected group
   touches all three of that realm's sides. A woven realm **seals**: its
   stones become immortal and the realm closes to further play.
4. **Win** by weaving **two** realms.

That is the entire ruleset. Everything else — eyes, ladders, ko fights,
sacrifices, walls, hunts — emerges.

**Why no draws:** the Y theorem (Schensted 1953) guarantees that a full
triangle has exactly one player connecting all three sides. Blocking your
opponent's Y is building your own — attack IS defense. Measured: 0% dead
positions across random fills (the earlier point-goal geometry: 53%).

**The trinity layer:** three realms share one turn clock. Every stone in
one realm concedes tempo in the other two. Losing realm one means the
fight moves — 1:1 makes the third realm the decider, and switching wars at
the right moment is the strategic skill single-board connection games
don't have.

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
