//! Supply War — deterministic realtime simulation core.
//!
//! Design: docs/design-supply-war-v2.md. This crate is pure logic:
//! no Bevy, no I/O. The world advances in fixed 100ms ticks; all
//! randomness comes from the map seed; player input is a command stream.
//! Replay = seed + commands.

// Archived lab prototype (built only under the client's supplywar-lab
// feature): the sim-style unwrap discipline of offline tooling applies,
// not the engine's no-panic rule — and its docs live in the design doc,
// not per-item rustdoc.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

pub mod field;
pub mod map;

pub use field::{Command, FieldState, LinkState, Outcome, TICKS_PER_SEC};
pub use map::{generate_map, MapSpec, SupplyMap};
