//! Trade system. OWNER: Opus B (docs/P2_CONTRATO.md).
use crate::world::World;
use ao_protocol::TradeC2S;

impl World {
    pub(crate) fn on_trade(&mut self, id: u32, msg: TradeC2S) {
        let _ = id;
        match msg {}
    }

    /// HOOK (interact, NPC system): the player double-clicked a merchant (`Comercia=1`)
    /// within range and alive. `IniciarComercioNPC`.
    pub(crate) fn open_npc_trade(&mut self, id: u32, npc: u32) {
        let _ = npc;
        self.console(id, "El comercio con NPCs llega en P2.", ao_protocol::Tone::Info);
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn trade_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    pub(crate) fn trade_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}
