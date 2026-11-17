//! Quest system. OWNER: Sonnet 1 (docs/P2_CONTRATO.md).
use crate::world::World;
use ao_protocol::QuestC2S;

impl World {
    pub(crate) fn on_quest(&mut self, id: u32, msg: QuestC2S) {
        let _ = id;
        match msg {}
    }

    /// HOOK (interact): the player talked to a quest NPC (`NpcType::Quest`, alive, near).
    pub(crate) fn quest_npc(&mut self, id: u32, npc: u32) {
        let _ = npc;
        self.console(id, "Las misiones llegan en P2.", ao_protocol::Tone::Info);
    }

    /// HOOK (combat): `killer` killed the NPC `npc_number` (npcs.dat) on `map`.
    pub(crate) fn quest_on_npc_killed(&mut self, killer: u32, npc_number: u16, map: u16) {
        let _ = (killer, npc_number, map);
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn quest_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    pub(crate) fn quest_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}
