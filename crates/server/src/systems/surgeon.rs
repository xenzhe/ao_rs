//! The surgeon NPC (TSAO `cirujano` + `frmCirujia`, handler `CABEZI`): pay to change the face.
//!
//! Fixes over TSAO: `CABEZI` checked neither the NPC, the distance nor death, so a forged
//! line changed the head anywhere; the client offered other ranges than the server
//! allowed. Here the window and the check come from the same table (`ao_formats::heads`,
//! also used at character creation) and the NPC is checked again on payment.
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_protocol::p4::surgeon::{SurgeonC2S, SurgeonS2C};
use ao_protocol::{Tone, S2C};

/// TSAO price.
pub const SURGERY_PRICE: u32 = 500;
const RANGE: i32 = 5;

impl World {
    fn surgeon_near(&self, id: u32, npc: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        self.instance(p.map).npcs.get(&npc).is_some_and(|n| n.kind == NpcType::Surgeon && n.alive() && (n.x - p.x).abs() + (n.y - p.y).abs() <= RANGE)
    }

    /// HOOK (interact).
    pub(crate) fn surgeon_open(&mut self, id: u32, npc: u32) {
        let Some(p) = self.player(id) else { return };
        if p.combat.dead {
            return self.console(id, "Estás muerto!!", Tone::Warn);
        }
        let heads: Vec<u16> = ao_formats::heads::for_recursos(&self.recursos).heads(p.race, p.gender).collect();
        let current = p.head;
        self.send(id, &S2C::Surgeon(SurgeonS2C::Open { npc, heads, price: SURGERY_PRICE, current }));
    }

    pub(crate) fn on_surgeon(&mut self, id: u32, m: SurgeonC2S) {
        let SurgeonC2S::Change { npc, head } = m;
        let close = |w: &World| w.send(id, &S2C::Surgeon(SurgeonS2C::Close));
        if !self.surgeon_near(id, npc) {
            self.console(id, "Estás demasiado lejos del cirujano.", Tone::Info);
            return close(self);
        }
        let p = self.player(id).unwrap();
        if p.combat.dead {
            self.console(id, "Estás muerto!!", Tone::Warn);
            return close(self);
        }
        if !ao_formats::heads::for_recursos(&self.recursos).allows(p.race, p.gender, head) {
            return self.strike(id, "cirujano: cabeza de otra raza");
        }
        if head == p.head {
            return self.console(id, "Ya tienes esa cara.", Tone::Info);
        }
        if p.inv.gold < SURGERY_PRICE {
            return self.console(id, format!("Necesitas {SURGERY_PRICE} monedas de oro."), Tone::Warn);
        }
        let p = self.player_mut(id);
        p.inv.gold -= SURGERY_PRICE;
        p.head = head;
        self.send_inventory_vitals(id);
        self.send_look(id);
        if let Some(p) = self.player(id) {
            self.save_player(p);
        }
        self.console(id, "El cirujano termina su trabajo. ¡Tienes una cara nueva!", Tone::Good);
        close(self);
    }
}
