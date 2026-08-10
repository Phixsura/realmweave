# Changelog

## Unreleased — 0.1.0 candidate

### Game
- **Triforce v5 织心** (flagship): one great triangle whose corners are
  the three realms and whose center is the weave-heart; two rules —
  weave (touch all three sides with one group; Y theorem: no draws) and
  death (liberties/capture/ko). Winning paths provably cross realms and
  contest the heart.
- **Trinity Y v4.1** (kept): three triangular realms; two rules — the Y
  goal (connect all three sides of a realm; first to two realms) and death
  (liberties/capture/ko). Won realms seal. Theory: the Y theorem gives
  attack-defense unity and guarantees decisiveness (0% dead random fills vs
  53% on the earlier point-goal geometry — see docs/experiments/audits/).
- weave-layers-v3: petrifying layer scoring with fossil roads (kept).
- weave-sever-v2, classic three-realms-v1 + sever variant (kept).
- Deleted dead experiments: doubleweave, territory, supply, supply-range.

### Engine & AI
- Deterministic replay across all rulesets (property-tested).
- MCTS engine for trinity (UCT over a dedicated fast simulator, 3000
  playouts/move default; 4/4 strength gate vs the 2-ply baseline).
- Criterion benchmarks for legal_moves / validate / replay.

### Client
- Bevy native client: 3D stacked realms + 2D analysis view, hot-seat,
  online rooms, replay viewer, interactive tutorial, vs-AI (worker-thread
  search, never blocks a frame), AI-vs-AI exhibition with commentary.

### Server
- Authoritative axum/tokio server: rooms, reconnect tokens, server clocks,
  SQLite event logs, structured tracing, enriched /healthz.

### Quality
- Workspace lint law: missing_docs + unwrap/expect bans (scoped, justified
  exceptions). Zero clippy warnings. 3-OS CI matrix + cargo-deny + release
  packaging workflow.
