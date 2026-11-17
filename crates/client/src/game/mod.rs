//! Client-side game state: what the server told us, plus local prediction.
//! - [`state`]: entities, stats, the `Game` struct and server message handling
//! - [`actions`]: what the player can do (walk, cast), mirroring server rules
pub mod actions;
pub mod state;

pub use state::{tile_center, Ent, Game, Screen};
