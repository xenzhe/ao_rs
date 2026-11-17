//! Faction system. OWNER: Opus C (docs/P2_CONTRATO.md).
use crate::world::World;
use ao_protocol::FactionC2S;

impl World {
    pub(crate) fn on_faction(&mut self, id: u32, msg: FactionC2S) {
        let _ = id;
        match msg {}
    }

    /// HOOK (interact): the player talked to a faction NPC (recruiter or governor).
    pub(crate) fn faction_npc(&mut self, id: u32, npc: u32, kind: ao_formats::dat::npcs::NpcType) {
        let _ = (npc, kind);
        self.console(id, "Las facciones llegan en P2.", ao_protocol::Tone::Info);
    }

    /// HOOK (combat, magic): why `attacker` may not hurt the player `victim`, if so
    /// (`PuedeAtacar` faction/criminal rules beyond safe zones). The caller shows the
    /// message to the attacker. Default: allowed.
    pub(crate) fn faction_attack_refusal(&self, attacker: u32, victim: u32) -> Option<&'static str> {
        let _ = (attacker, victim);
        None
    }

    /// HOOK (combat): `killer` killed the player `victim` (criminal status, faction
    /// kills...). Called after the death is processed.
    pub(crate) fn faction_on_player_killed(&mut self, killer: u32, victim: u32) {
        let _ = (killer, victim);
    }

    /// HOOK (combat): `killer` killed an NPC of npcs.dat `npc_number` (faction NPC kills).
    pub(crate) fn faction_on_npc_killed(&mut self, killer: u32, npc_number: u16) {
        let _ = (killer, npc_number);
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn faction_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    pub(crate) fn faction_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}
