//! Vault (bank). OWNER: bank system (P1, Sonnet 2). Opened by the NPC system through
//! `open_bank(player, banker_npc_id)` when a player interacts with a banker.
use crate::world::World;
use ao_protocol::Tone;

impl World {
    /// Called by the NPC system (interact) when the player talks to a banker.
    #[allow(dead_code)]
    pub(crate) fn open_bank(&mut self, id: u32, _banker: u32) {
        self.console(id, "La boveda todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_bank_deposit(&mut self, id: u32, _slot: u8, _amount: u32) {
        self.console(id, "La boveda todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_bank_withdraw(&mut self, id: u32, _slot: u8, _amount: u32) {
        self.console(id, "La boveda todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_bank_deposit_gold(&mut self, id: u32, _amount: u32) {
        self.console(id, "La boveda todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_bank_withdraw_gold(&mut self, id: u32, _amount: u32) {
        self.console(id, "La boveda todavia no esta disponible.", Tone::Warn);
    }
    pub(crate) fn on_bank_close(&mut self, _id: u32) {}
}
