# Board topology

## The hex family

Standard boards are **centered hexagonal** graphs: radius `r` gives
`1 + 3r(r+1)` nodes per realm.

| Board | Radius | Nodes/realm | Total |
|---|---|---|---|
| Small | 2 | 19 | 57 |
| Standard | 3 | 37 | 111 |
| Grand | 4 | 61 | 183 |

Each realm uses axial coordinates `(q, r)` with 6-neighbor adjacency, which
provides sixfold rotational and reflectional symmetry by construction. Node
ids are realm-major (Heaven block, Mortal block, Underworld block) with an
identical per-realm ordering, so `id % realm_size` is the cross-realm
correspondence used by validation.

## Gates (portals)

Not every node connects vertically. Cross-realm movement happens only at
**gate columns**, each linking Heaven↔Mortal and Mortal↔Underworld at the
same axial coordinate (adjacent realms only in v1).

Default `inner6-outer6` layout:

- 6 **inner gates**: the six ring-1 nodes.
- 6 **outer gates**: the six corners of ring `radius − 1`.
- On the radius-2 (19) board these coincide → 6 gate columns.

The portal layout is a generator parameter (`PortalSpec`), so alternative
counts and ring positions are data, not code. `realmweave-sim compare` runs
identical bot pairings on two boards to evaluate topology changes.

## Origins

Each player owns one outer-ring corner per realm, rotated 120° across realms
(Light: directions 0/2/4 for Heaven/Mortal/Underworld; Dark: the 180°
point-reflected pattern, directions 3/5/1). Origins never sit on gates.

## Validation (CI-enforced for every board file)

Structural: duplicate ids/edges, self edges, isolated nodes, portal edges
must link adjacent realms, exactly one origin per realm per player, no shared
origins, whole-graph connectivity.

Symmetry & fairness — never assumed from visuals:

- **Realm equivalence**: intra-realm edge sets must be isomorphic under the
  id correspondence.
- **Automorphism checks**: the 60° rotation and axis mirror maps must be
  graph automorphisms (boards with axial data).
- **Origin fairness**: both players' sorted multisets of pairwise
  origin distances and origin→gate distance profiles must be identical.

`realmweave-sim fairness` additionally reports degree histograms, minimum
vertex cuts / vertex-disjoint route counts between origin pairs (Menger), and
flags single-node bottlenecks ("super nodes").
