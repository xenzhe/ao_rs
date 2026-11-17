//! Party system. OWNER: Opus C (docs/P2_CONTRATO.md).
use crate::world::World;
use ao_protocol::PartyC2S;

impl World {
    pub(crate) fn on_party(&mut self, id: u32, msg: PartyC2S) {
        let _ = id;
        match msg {}
    }

    /// HOOK (combat): experience earned by `id` from an NPC on `map`. The party system
    /// shares it among nearby members (`CalcularDarExpGrupal`); alone it all goes to `id`.
    pub(crate) fn share_exp(&mut self, id: u32, amount: u32, map: u16) {
        let _ = map;
        self.give_exp(id, amount);
    }

    /// HOOK (chat channels): ids of the online members of `id`'s party, `id` included.
    /// Empty if not in a party.
    pub(crate) fn party_members_online(&self, id: u32) -> Vec<u32> {
        let _ = id;
        Vec::new()
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn party_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    pub(crate) fn party_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}
