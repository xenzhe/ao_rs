//! Emperor castles: port of `ModCastle.bas`. Using an emperor relic (`otCastleSpawner`,
//! obj 6362-6381, `AssignedCastleIndex`) on a foundation sign (trigger 41 + obj 6419 in
//! 21 official maps) builds the castle of that relic there: walls blocked, a doorway that
//! leads to its interior (maps 758-772) and four entry tiles stamped with the castle's
//! trigger (21-40). Only the owner's account and the characters on the whitelist
//! (`/CASTLE ADD nick`, `/CASTLE REMOVE nick`) may step on them. Moving a castle needs 7
//! days since it was founded. Clicking the model tells who founded it.
//!
//! Decisions: founding needs Patreon "Noble" in AO20; there is no Patreon here, so
//! `events.castle_without_patreon` decides (off by default: nobody, as the Patreon maps).
//! The castle model is a floor object over the sign (the sign is part of the map file).
//!
//! VB defects fixed:
//! - `IsEmperorCastleCreated(UserIndex, trigger)` matched the owner's account *or* the
//!   trigger and stopped at the first castle: the owner of castle 1 walked into every other
//!   castle. Entry is checked against the castle of that trigger only.
//! - `DoTileEvents` tested triggers 21-40 on every map, but the public maps use those
//!   numbers for roofs (776, 782, 795): only the four stamped tiles of a built castle count.
//! - Castles were saved only with the world save (a crash lost a new castle); now at once.
//! - `ModifyCastleEntryWhiteList` logged an invalid name and went on adding it.
use crate::db::CastleRow;
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::obj::ObjType;
use ao_protocol::{Tone, S2C};
use ao_rules::events::{self as rules, CASTLE_FOUNDATION_TRIGGER, CASTLE_MOCKUP_OBJ, CASTLE_RELOCATION_DAYS, CASTLE_SIGN_OBJ};
use std::collections::HashSet;

/// `t_CastleInfo`.
#[derive(Debug, Clone)]
pub struct Castle {
    pub row: CastleRow,
    pub whitelist: HashSet<String>,
}

#[derive(Debug, Default)]
pub struct Castles {
    pub list: Option<Vec<Castle>>,
}

impl World {
    /// `LoadCastleModule`: reads the database and builds the active castles.
    fn castles(&mut self) -> &mut Vec<Castle> {
        if self.castles.list.is_none() {
            let rows = self.db.castles().unwrap_or_default();
            let wl = self.db.castle_whitelists().unwrap_or_default();
            let list: Vec<Castle> = rows.into_iter().map(|row| Castle { whitelist: wl.iter().filter(|(c, _)| *c == row.id).map(|(_, n)| n.to_lowercase()).collect(), row }).collect();
            self.castles.list = Some(list);
            let active: Vec<usize> = (0..self.castles.list.as_ref().unwrap().len()).filter(|i| self.castles.list.as_ref().unwrap()[*i].row.active).collect();
            for i in active {
                self.castle_build(i);
            }
        }
        self.castles.list.as_mut().unwrap()
    }

    /// Loaded on the first tick (castles must stand before anyone walks there).
    pub(crate) fn tick_castles(&mut self) {
        if self.castles.list.is_none() {
            self.castles();
        }
    }

    /// `CreateCastleInMap` on the map tiles.
    fn castle_build(&mut self, i: usize) {
        let c = self.castles.list.as_ref().unwrap()[i].row.clone();
        let (Some((map, x, y)), inside) = (c.outside, c.inside) else { return };
        if !rules::castle_footprint_in_bounds(x, y) || !self.ensure_map(map) {
            return;
        }
        for xx in x - rules::CASTLE_NEG.0..=x + rules::CASTLE_POS.0 {
            for yy in y - rules::CASTLE_NEG.1..=y + rules::CASTLE_POS.1 {
                let t = ao_formats::csm::tile(xx, yy);
                let inst = self.maps.get_mut(&map).unwrap();
                inst.map.blocked[t] &= !ao_formats::csm::BLOCK_ALL;
                inst.map.triggers[t] = 0;
                if let Some(g) = inst.ground.at(xx, yy) {
                    self.erase_obj(map, xx, yy, g.amount);
                }
                if let Some(nid) = self.instance(map).npc_at(xx, yy) {
                    self.f6_remove_npc(map, nid);
                }
            }
        }
        let inst = self.maps.get_mut(&map).unwrap();
        for (bx, by) in rules::castle_blocked_tiles(x, y) {
            inst.map.blocked[ao_formats::csm::tile(bx, by)] |= ao_formats::csm::BLOCK_ALL;
        }
        inst.map.triggers[ao_formats::csm::tile(x, y)] = CASTLE_FOUNDATION_TRIGGER;
        for (ex, ey) in rules::castle_entry_tiles(x, y) {
            inst.map.triggers[ao_formats::csm::tile(ex, ey)] = c.trigger;
        }
        for (dx, dy) in rules::castle_door_tiles(x, y) {
            inst.map.set_exit(dx, dy, inside);
        }
        if let Some((im, ix, iy)) = inside {
            if self.ensure_map(im) {
                let m = &mut self.maps.get_mut(&im).unwrap().map;
                m.set_exit(ix, iy + 1, Some((map, x - 2, y + 1)));
                m.set_exit(ix + 1, iy + 1, Some((map, x - 1, y + 1)));
            }
        }
        self.make_obj(map, x, y, InvItem { obj: CASTLE_MOCKUP_OBJ, amount: 1 });
    }

    /// `DestroyCastleInMap`.
    fn castle_destroy(&mut self, i: usize) {
        let c = self.castles.list.as_ref().unwrap()[i].row.clone();
        let Some((map, x, y)) = c.outside else { return };
        if !self.ensure_map(map) {
            return;
        }
        if let Some(g) = self.maps[&map].ground.at(x, y) {
            self.erase_obj(map, x, y, g.amount);
        }
        let inst = self.maps.get_mut(&map).unwrap();
        for (bx, by) in rules::castle_blocked_tiles(x, y) {
            inst.map.blocked[ao_formats::csm::tile(bx, by)] &= !ao_formats::csm::BLOCK_ALL;
        }
        for (ex, ey) in rules::castle_entry_tiles(x, y) {
            inst.map.triggers[ao_formats::csm::tile(ex, ey)] = 0;
        }
        for (dx, dy) in rules::castle_door_tiles(x, y) {
            inst.map.set_exit(dx, dy, None);
        }
        inst.map.triggers[ao_formats::csm::tile(x, y)] = CASTLE_FOUNDATION_TRIGGER;
        if let Some((im, ix, iy)) = c.inside {
            if let Some(inst) = self.maps.get_mut(&im) {
                inst.map.set_exit(ix, iy + 1, None);
                inst.map.set_exit(ix + 1, iy + 1, None);
            }
        }
    }

    /// `IsValidCastlePosition` on the tile last clicked.
    fn castle_position_refusal(&mut self, id: u32) -> Result<(u16, i32, i32), &'static str> {
        const BAD: &str = "Posición de castillo inválida.";
        let p = self.player(id).ok_or(BAD)?;
        let (map, x, y) = p.target_tile.ok_or(BAD)?;
        if map != p.map || !rules::castle_footprint_in_bounds(x, y) {
            return Err(BAD);
        }
        if self.instance(map).map.trigger(x, y) != CASTLE_FOUNDATION_TRIGGER {
            return Err(BAD);
        }
        if self.instance(map).ground.at(x, y).is_some_and(|g| g.obj == CASTLE_MOCKUP_OBJ) {
            return Err("Ya hay un castillo en esta posición. No puedes fundar otro castillo aquí.");
        }
        if self.map_object_at(map, x, y) != Some(CASTLE_SIGN_OBJ) {
            return Err("No puedes fundar un castillo si no hay un cartel de fundación en la zona.");
        }
        Ok((map, x, y))
    }

    /// HOOK (inventory::on_use_item, `otCastleSpawner`): `CreateNewEmperorCastle`.
    pub(crate) fn castle_use_relic(&mut self, id: u32, obj: u16) {
        if !self.cfg.events.castle_without_patreon {
            return self.console(id, "Necesitas ser al menos patreon tier noble para usar este comando.", Tone::Warn);
        }
        let Some(index) = self.ao.objs.get(obj as u32).filter(|o| o.obj_type == ObjType::CastleSpawner).map(|o| o.castle_index) else { return };
        let (map, x, y) = match self.castle_position_refusal(id) {
            Ok(v) => v,
            Err(e) => return self.console(id, e, Tone::Warn),
        };
        let Some(i) = self.castles().iter().position(|c| c.row.id == index as i64) else { return };
        let account = self.sessions.get(&id).and_then(|s| s.account.as_ref()).map(|a| a.id);
        let name = self.name_of(id);
        let now = self.unix_now();
        // The owner moves their own castle (once a week); one castle per account.
        let owned = self.castles().iter().position(|c| c.row.active && (account.is_some() && c.row.owner_account == account || c.row.owner_name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&name))));
        if owned.is_some_and(|o| o != i) {
            return self.console(id, "Ya tienes un castillo.", Tone::Warn);
        }
        let c = &self.castles()[i].row;
        if c.active {
            if owned != Some(i) {
                return self.console(id, "Ese castillo ya tiene dueño.", Tone::Warn);
            }
            if now - c.founded.unwrap_or(0) < CASTLE_RELOCATION_DAYS * 86_400 {
                return self.console(id, "Debes esperar por lo menos una semana para reposicionar tu castillo.", Tone::Warn);
            }
            let old = c.name.clone().unwrap_or_default();
            self.castle_destroy(i);
            self.f6_say_all(format!("El castillo {old} de {name} ha sido destruido."));
        }
        let (a, b) = (self.rng.range(0, 19) as usize, self.rng.range(0, 19) as usize);
        let c = &mut self.castles()[i].row;
        c.outside = Some((map, x, y));
        c.founded = Some(now);
        c.active = true;
        c.owner_account = account;
        c.owner_name = Some(name.clone());
        c.name = Some(rules::castle_name(a, b));
        let row = c.clone();
        if let Err(e) = self.db.castle_save(&row) {
            tracing::warn!(?e, "no se pudo guardar el castillo");
        }
        self.castle_build(i);
        let cname = row.name.unwrap_or_default();
        self.f6_say_all(format!("El castillo {cname} de {name} ha sido posicionado a {map} ({x}-{y})."));
        for pid in self.player_ids() {
            if let Some(p) = self.player(pid) {
                self.send(pid, &S2C::Sound { wav: 247, x: p.x, y: p.y });
            }
        }
    }

    /// HOOK (movement, after a step): `CheckCastleEntryWhiteList`. True = sent back.
    pub(crate) fn castle_on_step(&mut self, id: u32, map: u16, x: i32, y: i32) -> bool {
        let t = self.instance(map).map.trigger(x, y);
        if !(rules::CASTLE_TRIGGER_FIRST..=rules::CASTLE_TRIGGER_LAST).contains(&t) {
            return false;
        }
        self.castles();
        let Some(c) = self.castles.list.as_ref().unwrap().iter().find(|c| c.row.trigger == t && c.row.active) else { return false };
        let Some((cm, cx, cy)) = c.row.outside else { return false };
        if cm != map || !rules::castle_entry_tiles(cx, cy).contains(&(x, y)) {
            return false;
        }
        if self.gm_rank(id) != ao_protocol::p4::gm::GmRank::User {
            return false;
        }
        let account = self.sessions.get(&id).and_then(|s| s.account.as_ref()).map(|a| a.id);
        let name = self.name_of(id);
        let allowed = (account.is_some() && c.row.owner_account == account) || c.row.owner_name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&name)) || c.whitelist.contains(&name.to_lowercase());
        if allowed {
            return false;
        }
        self.console(id, "No te encuentras en la lista de usuarios permitidos de este castillo.", Tone::Warn);
        self.f6_warp(id, map, x, y + 1);
        true
    }

    /// `/CASTLE ADD|REMOVE nick` (`HandleModifyCastleWhiteList`).
    pub(crate) fn castle_whitelist_cmd(&mut self, id: u32, rest: &str) {
        let mut it = rest.split_whitespace();
        let (op, who) = (it.next().map(str::to_lowercase), it.next().map(str::to_lowercase));
        let (Some(op), Some(who)) = (op, who) else { return self.console(id, "Uso: /CASTLE ADD <personaje> o /CASTLE REMOVE <personaje>", Tone::Info) };
        if op != "add" && op != "remove" {
            return self.console(id, "Uso: /CASTLE ADD <personaje> o /CASTLE REMOVE <personaje>", Tone::Info);
        }
        if who.len() > 30 || !who.chars().all(|c| c.is_alphanumeric() || c == ' ') {
            return self.console(id, "Nombre inválido.", Tone::Warn);
        }
        if !self.cfg.events.castle_without_patreon {
            return self.console(id, "Necesitas ser al menos patreon tier noble para usar este comando.", Tone::Warn);
        }
        let account = self.sessions.get(&id).and_then(|s| s.account.as_ref()).map(|a| a.id);
        let name = self.name_of(id);
        let Some(i) = self.castles().iter().position(|c| c.row.active && ((account.is_some() && c.row.owner_account == account) || c.row.owner_name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&name)))) else {
            return self.console(id, "No tienes ningún castillo.", Tone::Warn);
        };
        let cid = self.castles()[i].row.id;
        let add = op == "add";
        let changed = if add { self.castles()[i].whitelist.insert(who.clone()) } else { self.castles()[i].whitelist.remove(&who) };
        if changed {
            let _ = self.db.castle_whitelist_set(cid, &who, add);
            let text = if add { format!("El personaje {who} ha sido agregado satisfactoriamente a la lista.") } else { format!("El personaje {who} ha sido removido satisfactoriamente de la lista.") };
            self.console(id, text, Tone::Good);
        }
    }

    /// HOOK (interact, a click on a tile): `SendCastleInfo` for the castle model.
    pub(crate) fn castle_click(&mut self, id: u32, map: u16, x: i32, y: i32) -> bool {
        if !self.instance(map).ground.at(x, y).is_some_and(|g| g.obj == CASTLE_MOCKUP_OBJ) {
            return false;
        }
        self.castles();
        let Some(c) = self.castles.list.as_ref().unwrap().iter().find(|c| c.row.active && c.row.outside == Some((map, x, y))) else { return false };
        let date = c.row.founded.map(|t| {
            let d = rules::civil(t, self.cfg.events.utc_offset_hours);
            format!("{:02}/{:02}/{}", d.day, d.month, d.year)
        });
        let text = format!("Ves el castillo {} fundado en {} por {}.", c.row.name.clone().unwrap_or_default(), date.unwrap_or_default(), c.row.owner_name.clone().unwrap_or_default());
        self.console(id, text, Tone::Info);
        true
    }
}
