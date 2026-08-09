//! Supply War — deterministic realtime simulation core.
//!
//! Design: docs/design-supply-war-v2.md. This crate is pure logic:
//! no Bevy, no I/O. The world advances in fixed 100ms ticks; all
//! randomness comes from the map seed; player input is a command stream.
//! Replay = seed + commands.

pub mod field;
pub mod map;

pub use field::{Command, FieldState, LinkState, Outcome, TICKS_PER_SEC};
pub use map::{generate_map, MapSpec, SupplyMap};
