# Contributing to Realmweave

## Ground rules (non-negotiable)

1. **Determinism is sacred.** `GameRecord { config, moves }` must always
   reproduce the identical state via `Game::replay`. Every ruleset ships a
   replay test; the property suite (`tests/properties.rs`) enforces it
   under random play. No RNG in `realmweave-core` — seeds live in bots and
   tooling only.
2. **Core stays pure.** `realmweave-core` never grows a dependency on UI,
   networking, databases, or async runtimes. AI lives in `realmweave-bot`.
3. **Rules earn their place.** One ruleset per file in `core/src/rules/`.
   A new ruleset must state its design rationale in `docs/` and pass the
   duality standard where applicable (see `docs/experiments/audits/`).
   Dead experiments are deleted, not flag-gated — git history is the museum.
   The flagship (trinity-y-v4) has a hard discipline: two rules, no third.
4. **No panics in production paths.** `clippy::unwrap_used` is enforced;
   fail-fast is allowed only at binary startup and offline tooling, with a
   written per-file justification.
5. **Public API is documented.** `missing_docs` is enforced. Wire types and
   self-describing error enums may carry scoped allows with a reason.

## Workflow

```sh
cargo fmt --all                                    # formatting
cargo clippy --all-targets -- -D warnings          # zero warnings
cargo test --workspace                             # all tests incl. doctests
cargo run -p realmweave-cli -- validate boards/*.json
cargo bench -p realmweave-core --bench engine      # before/after for engine PRs
```

CI runs the above on Linux/macOS/Windows plus cargo-deny (advisories +
license policy) and a release build. All must be green.

## Measuring, not guessing

Game-design changes must come with harness evidence:
- `realmweave-bot/examples/trinityduel.rs` — self-play quality/length
- `realmweave-bot/examples/strength.rs` — engine strength gate
- `docs/experiments/audits/` — the duality measurement scripts that led to v4

Bot-play data never overrides human play-feel verdicts (see
`docs/design-trinity-y-v4.md` §5 for the discipline history).
