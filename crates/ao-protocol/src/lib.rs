//! Client/server protocol.
//!
//! - [`messages`]: what can be said (the client only sends intentions)
//! - [`framing`]: how it travels (length-prefixed bincode, with size limits)
//!
//! Bump [`PROTOCOL_VERSION`] on any change to the message definitions.
pub mod framing;
pub mod messages;

pub use framing::{encode, FrameError, FrameReader};
pub use messages::*;

pub const PROTOCOL_VERSION: u16 = 4;
/// Hard cap on a single client frame; anything bigger is an attack.
pub const MAX_CLIENT_FRAME: usize = 512;
/// Cap for server frames read by the client.
pub const MAX_SERVER_FRAME: usize = 1 << 20;
