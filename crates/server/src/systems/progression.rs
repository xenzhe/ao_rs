//! Attributes, skills, level-up gains, character creation values. OWNER: progression
//! system (P1, Sonnet 1).
use crate::world::World;
use ao_protocol::Tone;

impl World {
    /// Hook (contract): called by combat's `give_exp` after `levels` level-ups, to apply
    /// the class/race gains of Balance.dat (hp, mana, stamina, hit, skill points).
    #[allow(dead_code)]
    pub(crate) fn on_level_up(&mut self, _id: u32, _levels: u32) {}

    /// Hook (contract): fills a brand-new character's stats (attributes by race, initial
    /// hp/mana/stamina by class, starting skills, items, spells) before its first save.
    #[allow(dead_code)]
    pub(crate) fn init_new_character(&self, _race: u8, _class: u8, _gender: u8) -> Option<crate::db::CharacterState> {
        None
    }

    pub(crate) fn on_assign_skill(&mut self, id: u32, _skill: u8, _points: u16) {
        self.console(id, "Los skills todavia no estan disponibles.", Tone::Warn);
    }
}
