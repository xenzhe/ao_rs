//! The open transporter offer (from `S2C::Travel`) and the trip action.
use crate::Game;
use ao_protocol::p4::travel::{TravelC2S, TravelDest, TravelS2C};
use ao_protocol::C2S;

#[derive(Debug, Default)]
pub struct TravelState {
    /// `(npc, name, trips)` while a transporter window is open.
    pub open: Option<(u32, String, Vec<TravelDest>)>,
}

impl TravelState {
    pub fn apply(&mut self, msg: &TravelS2C) {
        match msg {
            TravelS2C::Open { npc, name, dests } => self.open = Some((*npc, name.clone(), dests.clone())),
            TravelS2C::Close => self.open = None,
        }
    }
}

impl Game {
    pub fn travel_go(&mut self, index: u8) {
        if let Some((npc, _, _)) = &self.local.p4.travel.open {
            let npc = *npc;
            self.send(&C2S::Travel(TravelC2S::Go { npc, index }));
        }
    }

    pub fn travel_close(&mut self) {
        self.local.p4.travel.open = None;
    }
}
