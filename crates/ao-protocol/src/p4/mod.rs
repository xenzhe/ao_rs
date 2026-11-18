//! P4 messages, one file per system owner (docs/P4_CONTRATO.md).
pub mod work;
pub mod craft;
pub mod gm;
pub mod event;
pub mod travel;
pub mod mail;
pub mod surgeon;
pub mod skill;
pub mod tsao;

pub use skill::SkillC2S;
pub use tsao::{TsaoC2S, TsaoS2C};

pub use surgeon::{SurgeonC2S, SurgeonS2C};

pub use travel::{TravelC2S, TravelS2C};
pub use mail::{MailC2S, MailS2C};
pub use work::{WorkC2S, WorkS2C};
pub use craft::{CraftC2S, CraftS2C};
pub use gm::{GmC2S, GmS2C};
pub use event::{EventC2S, EventS2C};
