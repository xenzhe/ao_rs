//! What the client knows about event (from `S2C::Event`) and its actions (`impl Game`).
//! OWNER: Opus D (docs/P4_CONTRATO.md).
use crate::Game;
use ao_protocol::p4::event::{AuctionLot, EventView, TournamentSpec};
use ao_protocol::{EventC2S, EventS2C, C2S};

#[derive(Debug, Default)]
pub struct EventState {
    /// Duels and tournament as the server last described them.
    pub view: EventView,
    /// Open auction lots.
    pub lots: Vec<AuctionLot>,
    /// Something arrived since the windows were last drawn (they may react, e.g. open).
    pub dirty: bool,
}

impl EventState {
    pub fn apply(&mut self, msg: &EventS2C) {
        match msg {
            EventS2C::State(v) => self.view = (**v).clone(),
            EventS2C::Auctions { lots } => self.lots = lots.clone(),
        }
        self.dirty = true;
    }

    /// Lot by id.
    pub fn lot(&self, id: u32) -> Option<&AuctionLot> {
        self.lots.iter().find(|l| l.id == id)
    }
}

/// Builds the AO20 player list of a duel (`Jugadores`): rivals at even positions, the
/// proposer's mates at odd ones. `rivals` must have one more name than `mates`.
pub fn duel_players(rivals: &[String], mates: &[String]) -> Option<Vec<String>> {
    let clean = |v: &[String]| v.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let (r, m) = (clean(rivals), clean(mates));
    if r.is_empty() || r.len() != m.len() + 1 {
        return None;
    }
    let mut out = Vec::new();
    for (i, rival) in r.into_iter().enumerate() {
        out.push(rival);
        if let Some(mate) = m.get(i) {
            out.push(mate.clone());
        }
    }
    Some(out)
}

impl Game {
    fn event(&mut self, m: EventC2S) {
        self.send(&C2S::Event(m));
    }

    pub fn event_refresh(&mut self) {
        self.event(EventC2S::Refresh);
    }

    /// 1v1: `rivals = [x]`; 2v2: `rivals = [x, y]`, `mates = [z]`.
    pub fn duel_propose(&mut self, rivals: &[String], mates: &[String], bet: u32, max_potions: i16) -> bool {
        match duel_players(rivals, mates) {
            Some(players) => {
                self.event(EventC2S::DuelPropose { players, bet, max_potions });
                true
            }
            None => {
                self.say("Indica tantos rivales como compañeros más uno.", ao_protocol::Tone::Warn);
                false
            }
        }
    }

    pub fn duel_accept(&mut self, offerer: &str) {
        self.event(EventC2S::DuelAccept { offerer: offerer.to_string() });
    }

    pub fn duel_cancel(&mut self) {
        self.event(EventC2S::DuelCancel);
    }

    pub fn duel_quit(&mut self) {
        self.event(EventC2S::DuelQuit);
    }

    pub fn auction_publish(&mut self, slot: u8, amount: u32, base_price: u32, minutes: u8) {
        self.event(EventC2S::AuctionPublish { slot, amount, base_price, minutes });
    }

    pub fn auction_bid(&mut self, lot: u32, amount: u32) {
        self.event(EventC2S::AuctionBid { lot, amount });
    }

    pub fn tournament_join(&mut self) {
        self.event(EventC2S::TournamentJoin);
    }

    pub fn tournament_leave(&mut self) {
        self.event(EventC2S::TournamentLeave);
    }

    pub fn tournament_open(&mut self, spec: TournamentSpec) {
        self.event(EventC2S::TournamentOpen(spec));
    }

    pub fn tournament_start(&mut self) {
        self.event(EventC2S::TournamentStart);
    }

    pub fn tournament_close(&mut self) {
        self.event(EventC2S::TournamentClose);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn duel_player_order() {
        assert_eq!(duel_players(&s(&["Rival"]), &[]), Some(s(&["Rival"])));
        assert_eq!(duel_players(&s(&["R1", "R2"]), &s(&["Mate"])), Some(s(&["R1", "Mate", "R2"])));
        assert_eq!(duel_players(&s(&["R1"]), &s(&["Mate"])), None);
        assert_eq!(duel_players(&s(&[" "]), &[]), None);
    }
}
