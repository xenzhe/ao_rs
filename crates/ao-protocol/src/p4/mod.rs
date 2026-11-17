//! P4 messages, one file per system owner (docs/P4_CONTRATO.md).
pub mod work;
pub mod craft;
pub mod gm;
pub mod event;

pub use work::{WorkC2S, WorkS2C};
pub use craft::{CraftC2S, CraftS2C};
pub use gm::{GmC2S, GmS2C};
pub use event::{EventC2S, EventS2C};
