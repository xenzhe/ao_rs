//! Health and mana regeneration, once per second.
use crate::world::World;
use ao_protocol::S2C;

impl World {
    pub(crate) fn regen(&mut self) {
        let (hp_regen, mana_regen) = (self.data.balance.hp_regen, self.data.balance.mana_regen);
        for id in self.player_ids() {
            let p = self.player_mut(id);
            let before = (p.hp, p.mana);
            p.hp = (p.hp + hp_regen).min(p.max_hp);
            p.mana = (p.mana + mana_regen).min(p.max_mana);
            if before == (p.hp, p.mana) {
                continue;
            }
            let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
            if before.0 != hp {
                self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
            }
            self.send_stats(id);
        }
    }
}
