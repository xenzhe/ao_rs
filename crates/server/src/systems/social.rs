//! P2 wiring kept by the integrator: enter/leave fan-out to every social system.
use crate::world::World;

impl World {
    pub(crate) fn social_on_enter(&mut self, id: u32) {
        self.faction_on_enter(id);
        self.guild_on_enter(id);
        self.party_on_enter(id);
        self.quest_on_enter(id);
        self.trade_on_enter(id);
    }

    pub(crate) fn social_on_leave(&mut self, id: u32) {
        self.trade_on_leave(id);
        self.party_on_leave(id);
        self.quest_on_leave(id);
        self.guild_on_leave(id);
        self.faction_on_leave(id);
    }
}
