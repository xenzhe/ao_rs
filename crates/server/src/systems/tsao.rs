//! Switch between the AO20 and the Tierras Sagradas combat rules (resource profile
//! `[Combat] Rules=tsao`) and the TSAO PvP balance (`Dat/pvp_balance.dat`).
use crate::world::World;
use ao_rules::tsao_combat::PvpBalance;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

impl World {
    pub(crate) fn tsao_rules(&self) -> bool {
        ao_formats::profile::for_recursos(&self.recursos).tsao_combat
    }

    /// TSAO `PoderEvasion` (+ `PoderEvasionEscudo` with a shield) of a fighter.
    pub(crate) fn tsao_evasion(&self, f: &super::combat::Fighter) -> i64 {
        use ao_formats::dat::balance::Skill;
        let mut e = ao_rules::tsao_combat::evasion_power(f.skill(Skill::Tactics), f.agility, f.level, f.mods.evasion);
        if let Some(s) = f.shield.and_then(|o| self.ao.objs.get(o as u32)) {
            e += ao_rules::tsao_combat::shield_evasion(f.agility, s.max_def, f.mods.shield);
        }
        e
    }

    /// Magic damage the victim's helmet, armour and ring absorb (modHechizos.bas: helmet and
    /// ring `RandomNumber(def - 3, def)` of `calcularDefCasco`, armour its plain range).
    pub(crate) fn tsao_magic_absorb(&mut self, f: &super::combat::Fighter) -> i64 {
        let ao = self.ao.clone();
        let def = |o: Option<u16>| o.and_then(|o| ao.objs.get(o as u32)).map(|d| d.tsao_magic_def).filter(|(a, b)| *a > 0 || *b > 0);
        let mut total = 0i64;
        for o in [f.helmet, f.tool] {
            // calcularDefCasco / calcularDefAnillo: the golden tiara and the ring of immunity
            // have a fixed value by class (`MagicDefByClass`) instead of their range.
            let fixed = self.class_magic_def(o, f.class.spanish_name());
            if let Some((lo, hi)) = fixed.map(|v| (v, v)).or_else(|| def(o)) {
                let d = self.roll(lo, hi);
                let d = self.roll(d - 2, d);
                total += self.roll(d - 3, d) as i64;
            }
        }
        if let Some((lo, hi)) = def(f.armor) {
            total += self.roll(lo, hi) as i64;
        }
        total.max(0)
    }

    /// TSAO spell damage of caster `id` (modHechizos.bas): `roll * 3.1`, the staff of a mage
    /// or druid on `StaffAffected` spells, then, against a player, the PvP class layer, the
    /// magic defence of helmet/armour/ring, and the weapon's `DañoMagico`.
    pub(crate) fn tsao_spell_damage(&mut self, id: u32, staff_affected: bool, roll: i32, victim: Option<u32>) -> i32 {
        use ao_formats::dat::balance::Class;
        let Some(fa) = self.fighter(id) else { return 0 };
        let weapon = fa.weapon.and_then(|o| self.ao.objs.get(o as u32)).cloned();
        let mut d = ao_rules::tsao_combat::spell_base(roll);
        if staff_affected {
            d = ao_rules::tsao_combat::staff_scaled(d, fa.class == Class::Mage, fa.class == Class::Druid, weapon.as_ref().map(|w| w.staff_damage_bonus));
        }
        if let Some(v) = victim.and_then(|v| self.fighter(v)) {
            // HechizoPropUsuario: the magic lute (`SpellDamagePct`) before the balance layer.
            let pct = self.spell_damage_pct(id) as f64;
            d = ao_rules::combat::vb_round(d as f64 + d as f64 * pct / 100.0);
            d = self.pvp_balance().magic(d, fa.class.spanish_name(), v.class.spanish_name());
            d -= self.tsao_magic_absorb(&v);
            // Class bonuses (modHechizos.bas `Bon2`/`Bon3`): caster's magic damage, victim's resistance.
            let tenths = self.tsao_bonus(id).magic_damage - victim.map_or(0, |v| self.tsao_bonus(v).magic_resist);
            d = super::tsao_progress::BonusMods::scale(d, tenths);
        }
        if let Some((lo, hi)) = weapon.map(|w| w.tsao_magic_damage).filter(|(lo, _)| *lo > 0) {
            d += self.roll(lo, hi) as i64;
        }
        d.clamp(0, i32::MAX as i64) as i32
    }

    /// Uses chest object in `slot` (TSAO otCofreAzar): its key (if it needs one) and the
    /// chest go, its contents come in (to the floor if the bag is full).
    pub(crate) fn open_chest(&mut self, id: u32, slot: u8, kind: u32, key: i32) {
        use crate::world::inventory::InvItem;
        use ao_protocol::Tone;
        static CHESTS: OnceLock<Mutex<HashMap<PathBuf, &'static HashMap<u32, ao_formats::dat::chests::Chest>>>> = OnceLock::new();
        let chests = *CHESTS.get_or_init(Default::default).lock().unwrap().entry(self.recursos.clone()).or_insert_with(|| Box::leak(Box::new(ao_formats::dat::chests::load(&self.recursos))));
        let Some(chest) = chests.get(&kind) else { return self.console(id, "Este cofre está vacío.", Tone::Warn) };
        if key > 0 {
            let Some(kslot) = self.player(id).and_then(|p| (1..=crate::world::inventory::MAX_INVENTORY_SLOTS as u8).find(|s| p.inv.get(*s).is_some_and(|i| i.obj as i32 == key))) else {
                let name = self.ao.objs.get(key as u32).map_or("la llave".to_string(), |o| o.name.clone());
                return self.console(id, format!("Necesitas {name} para abrir este cofre."), Tone::Warn);
            };
            self.remove_from_slot(id, kslot, 1);
        }
        let Some(class) = self.player(id).and_then(|p| ao_formats::dat::balance::Class::from_id(p.class as i32)) else { return };
        self.remove_from_slot(id, slot, 1);
        let contents = chest.open(class.spanish_name(), &mut |total| self.roll(1, total as i32) as u32);
        for (obj, amount) in contents {
            let Some(name) = self.ao.objs.get(obj as u32).map(|o| o.name.clone()) else { continue };
            let item = InvItem { obj, amount };
            if !self.add_item(id, item) {
                let (m, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
                self.drop_to_floor(m, x, y, item, false);
            }
            self.console(id, format!("Has obtenido {amount} {name}."), Tone::Good);
        }
    }

    /// The PvP layer, read once per resource folder (empty without the file).
    pub(crate) fn pvp_balance(&self) -> &'static PvpBalance {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, &'static PvpBalance>>> = OnceLock::new();
        let mut m = CACHE.get_or_init(Default::default).lock().unwrap();
        m.entry(self.recursos.clone()).or_insert_with(|| {
            let bytes = std::fs::read(self.recursos.join("Dat").join("pvp_balance.dat")).unwrap_or_default();
            let text: String = bytes.iter().map(|&b| b as char).collect();
            Box::leak(Box::new(PvpBalance::parse(&text)))
        })
    }
}
