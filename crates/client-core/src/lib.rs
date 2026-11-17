//! Client logic without any graphics library, shared by the game client (macroquad
//! frontend) and the headless test bots (`ao-testkit`).
//!
//! - [`state`]: entities, stats, the [`Game`] struct and server message handling
//! - [`actions`]: what the player can do (walk, cast, chat), mirroring server rules
//! - [`events`]: what the frontend should show (effects, floating text...)
//! - [`net`]: non-blocking TCP; [`guard`]: obfuscated stats
//!
//! Time is injected: the frontend passes its frame clock, a bot passes the server's
//! virtual clock. Nothing here reads the OS clock.
pub mod actions;
pub mod events;
pub mod guard;
pub mod local;
pub mod net;
pub mod p3;
pub mod p4;
pub mod social;
pub mod state;

pub use events::{ClientEvent, Pt};
pub use local::{ItemSlot, Local};
pub use state::{tile_center, Ent, Game, Screen, Stats, TILE};
