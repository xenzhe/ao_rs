//! Guild system. OWNER: Opus A (docs/P2_CONTRATO.md).
use crate::world::World;
use ao_protocol::GuildC2S;

impl World {
    pub(crate) fn on_guild(&mut self, id: u32, msg: GuildC2S) {
        let _ = id;
        match msg {}
    }

    /// HOOK (chat channels): ids of the online members of `id`'s guild, `id` included.
    /// Empty if not in a guild.
    pub(crate) fn guild_members_online(&self, id: u32) -> Vec<u32> {
        let _ = id;
        Vec::new()
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn guild_on_enter(&mut self, id: u32) {
        let _ = id;
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    pub(crate) fn guild_on_leave(&mut self, id: u32) {
        let _ = id;
    }
}
