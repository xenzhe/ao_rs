//! Spell casting: every check happens here, the client only names the spell and the tile.
use crate::world::World;
use ao_protocol::{Tone, S2C};
use ao_rules::spells::SpellTarget;
use ao_rules::{tile_distance, walk};

impl World {
    pub(crate) fn on_cast(&mut self, id: u32, spell_id: u16, tx: i32, ty: i32, seq: u32) {
        let Some(spell) = self.data.spell(spell_id).cloned() else {
            return self.strike(id, "hechizo inexistente");
        };
        let now = self.now();
        let (map, px, py, seq_ok, cd_ok, mana_ok, level) = {
            let p = self.player_mut(id);
            let seq_ok = p.seq.accept(seq);
            let cd_ok = p.cooldowns.get(&spell.id).is_none_or(|t| now >= *t);
            (p.map, p.x, p.y, seq_ok, cd_ok, p.mana >= spell.mana, p.level)
        };
        if !seq_ok {
            return self.strike(id, "paquete repetido (replay)");
        }
        if !cd_ok {
            self.console(id, "Debes esperar para volver a lanzar ese hechizo.", Tone::Warn);
            return self.strike(id, "hechizo antes del intervalo");
        }
        if !mana_ok {
            return self.console(id, "No tienes suficiente mana.", Tone::Warn);
        }
        if tile_distance((px, py), (tx, ty)) > spell.range {
            return self.console(id, "Estas demasiado lejos.", Tone::Warn);
        }
        if !walk::line_of_sight(&self.instance(map).map, px, py, tx, ty) {
            return self.console(id, "No hay linea de vision.", Tone::Warn);
        }

        let amount = spell.base_amount(level) * self.rng.range(85, 115) / 100;
        match spell.target {
            SpellTarget::Enemy => {
                let Some(nid) = self.instance(map).npc_at(tx, ty) else {
                    let msg = if self.player_at(map, tx, ty).is_some() { "No puedes atacar a otros jugadores aqui." } else { "No hay ningun objetivo ahi." };
                    return self.console(id, msg, Tone::Warn);
                };
                self.spend(id, spell.id, spell.mana, spell.cooldown_ms);
                self.broadcast(map, &S2C::Effect { spell: spell.id, from: id, tx, ty }, None);
                self.damage_npc(map, nid, amount, id);
            }
            SpellTarget::Ally => {
                let Some(pid) = self.player_at(map, tx, ty) else {
                    return self.console(id, "Ese hechizo solo funciona sobre personajes.", Tone::Warn);
                };
                self.spend(id, spell.id, spell.mana, spell.cooldown_ms);
                self.broadcast(map, &S2C::Effect { spell: spell.id, from: id, tx, ty }, None);
                let healed = self.heal_player(pid, amount);
                self.console(id, format!("Has restaurado {healed} puntos de vida."), Tone::Good);
            }
        }
        self.send_stats(id);
    }

    fn spend(&mut self, id: u32, spell: u16, mana: i32, cooldown_ms: u64) {
        let now = self.now();
        let p = self.player_mut(id);
        p.mana -= mana;
        p.cooldowns.insert(spell, now.after_ms(cooldown_ms));
    }
}
