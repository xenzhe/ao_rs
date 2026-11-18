//! Map entry rules (`GameLogic.bas::CheckMapRestrictions`): newbie-only maps, citizens or
//! criminals only, clans only, groups only and level ranges, read from each map's
//! `restrict_mode` and `level` (`AoMap::restrict`, `min_level`, `max_level`).
use crate::world::World;
use ao_formats::csm::{RESTRICT_CLANS_ONLY, RESTRICT_GROUPS_ONLY, RESTRICT_NEWBIE, RESTRICT_NO_CITIZENS, RESTRICT_NO_CRIMINALS, RESTRICT_PATREON_ONLY};
use ao_protocol::p4::gm::GmRank;

impl World {
    /// Why `id` may not enter `map` (None = allowed). GMs always may.
    pub(crate) fn map_entry_refusal(&mut self, id: u32, map: u16) -> Option<String> {
        if self.gm_rank(id) != GmRank::User {
            return None;
        }
        if !self.ensure_map(map) {
            return None;
        }
        let (restrict, min_level, max_level) = {
            let m = &self.instance(map).map;
            (m.restrict, m.min_level, m.max_level)
        };
        let p = self.player(id)?;
        let level = p.level;
        let in_guild = p.guild.guild.is_some();
        let has = |bit: u32| restrict & bit != 0;
        if has(RESTRICT_NEWBIE) && level > self.data.balance.newbie_max_level {
            return Some("Sólo los newbies pueden entrar a este mapa.".into());
        }
        // Status: 0 criminal, 1 citizen, 2 chaos legion, 3 royal army.
        let status = self.faction_status(id) as i32;
        if has(RESTRICT_NO_CRIMINALS) && (status == 0 || status == 2) {
            return Some("Sólo los ciudadanos pueden entrar a este mapa.".into());
        }
        if has(RESTRICT_NO_CITIZENS) && (status == 1 || status == 3) {
            return Some("Sólo los criminales pueden entrar a este mapa.".into());
        }
        if has(RESTRICT_CLANS_ONLY) && !in_guild {
            return Some("Necesitas pertenecer a un clan para entrar a este mapa.".into());
        }
        if min_level != 0 && level < min_level {
            return Some(format!("Necesitas ser al menos nivel {min_level}."));
        }
        if max_level != 0 && level >= max_level {
            return Some(format!("Sólo los personajes inferiores a nivel {max_level}."));
        }
        if has(RESTRICT_GROUPS_ONLY) && self.party_of(id).is_none() {
            return Some("Necesitas pertenecer a un grupo para entrar a este mapa.".into());
        }
        // There is no Patreon here: those maps stay closed to players.
        if has(RESTRICT_PATREON_ONLY) {
            return Some("Necesitas ser Patreon para entrar a este mapa.".into());
        }
        None
    }
}
