//! P4 client state, one file per owner (docs/P4_CONTRATO.md).
pub mod work;
pub mod craft;
pub mod gm;
pub mod event;
pub mod travel;

use ao_protocol::S2C;

#[derive(Debug, Default)]
pub struct P4 {
    pub work: work::WorkState,
    pub craft: craft::CraftState,
    pub gm: gm::GmState,
    pub event: event::EventState,
    pub travel: travel::TravelState,
}

impl P4 {
    /// Applies a P4 message. Returns true if it was one.
    pub fn apply(&mut self, msg: &S2C) -> bool {
        match msg {
            S2C::Work(m) => self.work.apply(m),
            S2C::Craft(m) => self.craft.apply(m),
            S2C::Gm(m) => self.gm.apply(m),
            S2C::Event(m) => self.event.apply(m),
            S2C::Travel(m) => self.travel.apply(m),
            _ => return false,
        }
        true
    }
}
