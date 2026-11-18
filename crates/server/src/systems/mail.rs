//! Player mail at a post office NPC (TSAO `AA_Correos.bas` + `frmCorreo`).
//!
//! Fixes over TSAO:
//! - the VB took the parcel by inventory slot number from the client text and only checked
//!   `TieneObjetos` per object, then removed items one by one: an error halfway (its
//!   handler just logged) lost what was already removed, and repeated slots could ask for
//!   more than the player had in total. Here the whole parcel is validated first (each slot
//!   once, amount owned, not equipped, transferable, not newbie), then taken in one go;
//! - recipients were `.chr` files written by any sender while the owner was online (lost
//!   letters, overwritten counters). Mail lives in its own table and is read on demand;
//! - a parcel could be claimed twice from two clients; the DB update now takes it once;
//! - the sender is saved before the letter exists: a crash in between can lose a parcel
//!   but never duplicate it.
use crate::db::{now_unix, MailLetter};
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_protocol::p4::mail::{MailC2S, MailHeader, MailLetterView, MailS2C, BODY_MAX, PARCEL_MAX, SUBJECT_MAX};
use ao_protocol::{Tone, S2C};
use tracing::{info, warn};

/// Letters an inbox holds (TSAO: 30).
pub const INBOX_MAX: usize = 30;
/// Gold a letter costs (keeps the post office from being a spam channel).
pub const POSTAGE: u32 = 100;
const RANGE: i32 = 5;
const GOLD_OBJ: u16 = 12;

impl World {
    /// The player stands within reach of post office `npc` (alive) on its map.
    fn mail_near(&self, id: u32, npc: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        if p.hp <= 0 {
            self.console(id, "Estás muerto!!", Tone::Warn);
            return false;
        }
        let ok = self.instance(p.map).npcs.get(&npc).is_some_and(|n| n.kind == NpcType::PostOffice && (n.x - p.x).abs() + (n.y - p.y).abs() <= RANGE);
        if !ok {
            self.console(id, "Estás demasiado lejos del correo.", Tone::Info);
            self.send(id, &S2C::Mail(MailS2C::Close));
        }
        ok
    }

    /// HOOK (interact): opens the post office window with the inbox.
    pub(crate) fn mail_npc(&mut self, id: u32, npc: u32) {
        let Some(name) = self.player(id).map(|p| p.name.clone()) else { return };
        let letters = match self.db.mail_inbox(&name) {
            Ok(l) => l,
            Err(e) => return warn!(?e, "correo: no se pudo leer el buzón"),
        };
        let letters = letters
            .into_iter()
            .map(|l| MailHeader { id: l.id, parcel: l.gold > 0 || !l.items.is_empty(), sender: l.sender, subject: l.subject, sent_at: l.sent_at, unread: l.unread })
            .collect();
        self.send(id, &S2C::Mail(MailS2C::Inbox { npc, letters, postage: POSTAGE, capacity: INBOX_MAX as u8 }));
    }

    /// HOOK (session): tells a player entering the world about unread letters.
    pub(crate) fn mail_on_enter(&self, id: u32) {
        let Some(name) = self.player(id).map(|p| p.name.clone()) else { return };
        if let Ok(n) = self.db.mail_unread(&name) {
            if n > 0 {
                self.console(id, format!("Tienes {n} carta{} sin leer en el correo.", if n == 1 { "" } else { "s" }), Tone::Good);
            }
        }
    }

    pub(crate) fn on_mail(&mut self, id: u32, m: MailC2S) {
        match m {
            MailC2S::Send { npc, to, subject, body, gold, items } => {
                if self.mail_near(id, npc) {
                    self.mail_send(id, npc, to, subject, body, gold, items);
                }
            }
            MailC2S::Read { npc, id: letter } => {
                if self.mail_near(id, npc) {
                    self.mail_read(id, letter);
                }
            }
            MailC2S::Take { npc, id: letter } => {
                if self.mail_near(id, npc) {
                    self.mail_take(id, npc, letter);
                }
            }
            MailC2S::Delete { npc, id: letter } => {
                if self.mail_near(id, npc) {
                    let name = self.player(id).unwrap().name.clone();
                    match self.db.mail_delete(&name, letter) {
                        Ok(true) => self.mail_npc(id, npc),
                        Ok(false) => self.console(id, "Retira primero lo que trae la carta.", Tone::Warn),
                        Err(e) => warn!(?e, "correo: no se pudo borrar"),
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn mail_send(&mut self, id: u32, npc: u32, to: String, subject: String, body: String, gold: u32, items: Vec<(u8, u32)>) {
        let refuse = |w: &World, msg: &str| w.console(id, msg, Tone::Warn);
        let to = to.trim().to_string();
        let subject: String = subject.trim().chars().filter(|c| !c.is_control()).take(SUBJECT_MAX).collect();
        let body: String = body.trim().chars().filter(|c| *c == '\n' || !c.is_control()).take(BODY_MAX).collect();
        let p = self.player(id).unwrap();
        let me = p.name.clone();
        if to.is_empty() || subject.is_empty() {
            return refuse(self, "Escribe el destinatario y el asunto.");
        }
        if to.eq_ignore_ascii_case(&me) {
            return refuse(self, "No puedes enviarte cartas a ti mismo.");
        }
        if p.trade.with.is_some() || p.trade.shop.is_some() {
            return refuse(self, "No puedes enviar correo mientras comercias.");
        }
        if items.len() > PARCEL_MAX {
            return self.strike(id, "correo: demasiados objetos");
        }
        // The whole parcel is checked before anything is taken.
        let mut parcel: Vec<(u8, InvItem)> = Vec::new();
        for (slot, amount) in &items {
            let Some(it) = p.inv.get(*slot) else { return refuse(self, "Ese objeto ya no está en tu inventario.") };
            if *amount == 0 || *amount > it.amount || parcel.iter().any(|(s, _)| s == slot) {
                return self.strike(id, "correo: cantidad o casilla invalida");
            }
            if p.inv.equipped.is_equipped(*slot) || p.inv.pending_equipped.contains(slot) {
                return refuse(self, "Desequipa el objeto antes de enviarlo.");
            }
            let Some(d) = self.ao.objs.get(it.obj as u32) else { return };
            if d.non_transferable == 1 || d.newbie == 1 {
                return refuse(self, &format!("No puedes enviar {}: es intransferible.", d.name));
            }
            parcel.push((*slot, InvItem { obj: it.obj, amount: *amount }));
        }
        let cost = gold as u64 + POSTAGE as u64;
        if (p.inv.gold as u64) < cost {
            return refuse(self, &format!("Necesitas {cost} monedas de oro (incluye {POSTAGE} del envío)."));
        }
        let recipient = match self.db.character_name(&to) {
            Ok(Some(n)) => n,
            Ok(None) => return refuse(self, "Ese personaje no existe."),
            Err(e) => return warn!(?e, "correo: consulta de destinatario"),
        };
        match self.db.mail_count(&recipient) {
            Ok(n) if n >= INBOX_MAX => return refuse(self, "El buzón de ese personaje está lleno."),
            Ok(_) => {}
            Err(e) => return warn!(?e, "correo: consulta de buzón"),
        }
        // Take everything, save the sender, then post the letter.
        for (slot, it) in &parcel {
            self.remove_from_slot(id, *slot, it.amount);
        }
        self.player_mut(id).inv.gold -= cost as u32;
        self.send_inventory_vitals(id);
        if let Some(p) = self.player(id) {
            self.save_player(p);
        }
        let letter = MailLetter { id: 0, sender: me.clone(), subject, body, gold, items: parcel.iter().map(|(_, i)| (i.obj, i.amount)).collect(), sent_at: now_unix(), unread: true };
        if let Err(e) = self.db.mail_send(&recipient, &letter) {
            warn!(?e, from = %me, to = %recipient, "correo: no se pudo guardar la carta");
            return refuse(self, "El correo no pudo enviar la carta.");
        }
        info!(from = %me, to = %recipient, gold, items = ?letter.items, "correo enviado");
        self.send(id, &S2C::Mail(MailS2C::Sent));
        self.console(id, format!("Carta enviada a {recipient}."), Tone::Good);
        if let Some(rid) = self.id_by_name(&recipient) {
            self.console(rid, format!("Te ha llegado una carta de {me}."), Tone::Good);
        }
        self.mail_npc(id, npc);
    }

    fn mail_read(&mut self, id: u32, letter: i64) {
        let name = self.player(id).unwrap().name.clone();
        let Ok(Some(l)) = self.db.mail_get(&name, letter) else { return };
        if l.unread {
            let _ = self.db.mail_mark_read(&name, letter);
        }
        self.send(id, &S2C::Mail(MailS2C::Letter(MailLetterView { id: l.id, sender: l.sender, subject: l.subject, body: l.body, sent_at: l.sent_at, gold: l.gold, items: l.items })));
    }

    /// All or nothing: the bag must have room for every stack.
    fn mail_take(&mut self, id: u32, npc: u32, letter: i64) {
        let name = self.player(id).unwrap().name.clone();
        let Ok(Some(l)) = self.db.mail_get(&name, letter) else { return };
        if l.gold == 0 && l.items.is_empty() {
            return;
        }
        if !self.fits_in_bag(id, &l.items) {
            return self.console(id, "No tienes espacio en el inventario para todo el paquete.", Tone::Warn);
        }
        match self.db.mail_take_parcel(&name, letter) {
            Ok(true) => {}
            Ok(false) => return,
            Err(e) => return warn!(?e, "correo: no se pudo retirar"),
        }
        if l.gold > 0 {
            self.add_item(id, InvItem { obj: GOLD_OBJ, amount: l.gold });
        }
        for (obj, amount) in &l.items {
            self.add_item(id, InvItem { obj: *obj, amount: *amount });
        }
        if let Some(p) = self.player(id) {
            self.save_player(p);
        }
        info!(to = %name, from = %l.sender, gold = l.gold, items = ?l.items, "correo retirado");
        self.console(id, "Has retirado el paquete.", Tone::Good);
        self.mail_read(id, letter);
        self.mail_npc(id, npc);
    }

    /// Would these stacks all fit (merging into stacks with room, then empty slots)?
    pub(crate) fn fits_in_bag(&self, id: u32, items: &[(u16, u32)]) -> bool {
        use ao_rules::items::MAX_STACK;
        let Some(p) = self.player(id) else { return false };
        let n = p.inv.usable_slots();
        let mut cells: Vec<Option<InvItem>> = (1..=n).map(|s| p.inv.get(s)).collect();
        for (obj, amount) in items.iter().filter(|(o, _)| *o != GOLD_OBJ) {
            if let Some(c) = cells.iter_mut().flatten().find(|c| c.obj == *obj && c.amount + amount <= MAX_STACK) {
                c.amount += amount;
            } else if let Some(c) = cells.iter_mut().find(|c| c.is_none()) {
                *c = Some(InvItem { obj: *obj, amount: *amount });
            } else {
                return false;
            }
        }
        true
    }
}
