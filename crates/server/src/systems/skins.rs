//! Skins (`SkinEquip` / `DesequiparSkin`): objects that only change how a piece looks
//! (armour body, helmet or head, shield, weapon, boat). Here they live in the normal
//! inventory (the official server keeps them in a separate Patreon inventory); using one
//! wears it or takes it off, one per kind. Wings and spell skins have no visible layer on
//! this client and only stay equipped.
use crate::world::World;
use ao_formats::dat::obj::{ObjData, ObjType};
use ao_protocol::Tone;

/// Index in `Equipped::skins` of each skin kind.
pub fn skin_kind(t: ObjType) -> Option<usize> {
    Some(match t {
        ObjType::SkinsArmours => 0,
        ObjType::SkinsHelmets => 1,
        ObjType::SkinsShields => 2,
        ObjType::SkinsWeapons => 3,
        ObjType::SkinsBoats => 4,
        ObjType::SkinsWings => 5,
        ObjType::SkinsSpells => 6,
        _ => return None,
    })
}

/// What the worn skins change in a look: (body, head, weapon, shield, helmet), each None
/// when no skin of that kind applies.
pub struct SkinLook {
    pub body: Option<u16>,
    pub head: Option<u16>,
    pub weapon: Option<u16>,
    pub shield: Option<u16>,
    pub helmet: Option<u16>,
}

impl World {
    /// `SkinEquip` / `DesequiparSkin` from using the object.
    pub(crate) fn toggle_skin(&mut self, id: u32, slot: u8, logging_in: bool) {
        let ao = self.ao.clone();
        let Some(o) = self.player(id).and_then(|p| p.inv.get(slot)).and_then(|it| ao.objs.get(it.obj as u32)) else { return };
        let Some(kind) = skin_kind(o.obj_type) else { return };
        let p = self.player_mut(id);
        let worn = &mut p.inv.equipped.skins[kind];
        if *worn == Some(slot) {
            if logging_in {
                return;
            }
            *worn = None;
            self.console(id, "Te has quitado el aspecto.", Tone::Info);
        } else {
            *worn = Some(slot);
            if !logging_in {
                self.console(id, format!("Ahora usas el aspecto {}.", o.name), Tone::Good);
            }
        }
        self.send_slot(id, slot);
        if !logging_in {
            self.send_look(id);
        }
    }

    /// The skins worn by `id` (ObtenerRopaje for armour and boat bodies).
    pub(crate) fn skin_look(&self, id: u32) -> SkinLook {
        let mut l = SkinLook { body: None, head: None, weapon: None, shield: None, helmet: None };
        let Some(p) = self.player(id) else { return l };
        let skin = |k: usize| -> Option<&ObjData> { p.inv.equipped.skins[k].and_then(|s| p.inv.get(s)).and_then(|it| self.ao.objs.get(it.obj as u32)) };
        let body_of = |o: &ObjData| {
            let b = ao_rules::items::clothing_for(o, p.race, p.gender);
            (b > 0).then_some(b as u16).or((o.clothing > 0).then_some(o.clothing as u16))
        };
        if p.combat.dead || p.mounted.is_some() {
            return l;
        }
        if p.status.sailing.is_some() {
            l.body = skin(4).and_then(body_of);
            return l;
        }
        l.body = skin(0).and_then(body_of);
        if let Some(h) = skin(1).filter(|h| h.helmet_anim > 0) {
            // Subtipo 2 replaces the head; the rest is a helmet drawn over it.
            if h.subtype == 2 {
                l.head = Some(h.helmet_anim as u16);
                l.helmet = Some(0);
            } else {
                l.helmet = Some(h.helmet_anim as u16);
            }
        }
        l.shield = skin(2).filter(|s| s.shield_anim > 0).map(|s| s.shield_anim as u16);
        l.weapon = skin(3).filter(|w| w.weapon_anim > 0).map(|w| w.weapon_anim as u16);
        l
    }
}
