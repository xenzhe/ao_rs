//! Tierras Sagradas progression: the three currencies (tournament points, TS points,
//! donation credits), where they come from, the exchange, the class bonuses of levels
//! 53/56/60, marriage and the popular vote. All the numbers come from the data
//! (tools/tsao/progress.py, `ao_formats::dat::tsao_progress`); an AO20 resource set has none
//! of these files and none of this runs.
//!
//! Ported from:
//! - points: `Stats.PuntosTorneo` / `TSPoints` / `PuntosDonacion` (Declares.bas, FileIO.bas),
//!   `AgregarPuntos` (Acciones.bas), `NPCDarPuntos` (MODULO_NPCs.bas), the level milestones of
//!   `CheckUserLevel` (Modulo_UsUaRiOs.bas) and the quest rewards (modQuests.bas);
//! - exchange: `CCANJE` / `IPX` / `SPX` (TCP_HandleData1.bas, Premios.dat), `FTSPTS`
//!   (TCP_HandleData1.bas), `DCANJE` / `DPX` / `DRX` (TCP_HandleData4.bas, ItemsDonaciones.dat);
//! - class bonus: `CheckUserLevel` message `99`, `BOF` (TCP_HandleData1.bas), the `Bon1..3`
//!   checks of SistemaCombate.bas / modHechizos.bas / Trabajo.bas;
//! - marriage: `/CASAR`, `/DIVORCIARSE` (TCP_HandleData3.bas), "<Matrimonio con X>" (GameLogic.bas);
//! - vote: `/ENCUESTA` (TCP.bas), `/VOTAR` and `/RESULTADOS` (TCP_HandleData2.bas),
//!   consultas.dat (ConsultasPopulares.cls).
//!
//! Fixes over TSAO:
//! - `SPX` checked the amount with `And` (`<= 0 And > 10000`, never true), accepted prize 0
//!   and multiplied price by amount in 16 bits: any amount, overflowing costs. Here
//!   1..=10000 units, prices in 64 bits, and the bag is checked before paying;
//! - `DRX` gave the pack's objects one by one and left without charging when one did not fit
//!   (free objects), took points for skins/premium it could not give, and never checked the
//!   pack index. Here the pack is validated whole (supported rewards, room for every stack),
//!   then charged, then given;
//! - `BOF` took the bonus TEXT from the client and never checked the tier was free: sending
//!   "Aumenta en 5 puntos tu vida." again at level 53 gave 5 more HP each time. The server
//!   now offers a tier, the client answers 0/1, and the choice is stored once per tier
//!   (offered again on entering if the player logged out without choosing);
//! - the evasion, shield and hit bonuses added 0.04 to integer powers in the hundreds (no
//!   effect); they are 4% here. "Aumenta tu daño con flechas." had no code: 4% arrow damage;
//! - the user-kill quest doubled the tournament/TS points with `IIf(estado, x, x * 2)`, the
//!   opposite of the NPC quest: no doubling here (there is no premium);
//! - `/RESULTADOS` divided by the total with no votes (error), votes lived in memory and
//!   `/VOTAR` could be answered several times from one account; votes are stored, one per
//!   account, and results show once you voted;
//! - `SPÑ` (Mejorados.dat) took the upgrade by NAME from the client text, gave the new
//!   object before taking the old one and could take an equipped one; here by index, only
//!   unequipped objects, the gem found by its TSAO type (not object 1448 in code);
//! - `/DIVORCIARSE` read the spouse from the file after clearing it (the spouse's file was
//!   never cleared when offline); marriages are a table and both sides go together.
use crate::db::tsao::PointsRow;
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::tsao_progress::{BonusEffect, PackReward, TsaoProgress};
use ao_protocol::p4::gm::GmRank;
use ao_protocol::p4::tsao::{PackView, Points, PrizeView, TsView, TsaoC2S, TsaoS2C};
use ao_protocol::{Tone, S2C};
use std::sync::Arc;
use tracing::{info, warn};

/// Most units of a prize bought at once (TSAO meant 10000 with its broken check).
pub const MAX_PRIZE_UNITS: u32 = 10_000;
/// TSAO `otGemaOctarina` (Declares.bas), kept by the converter as `TsaoObjType`.
pub const TSAO_OCTARINE_GEM: i32 = 37;

/// A player's TSAO progression piece (loaded on entering, written on every change).
#[derive(Clone, Debug, Default)]
pub struct TsaoPiece {
    pub points: PointsRow,
    /// `(tier, option)` chosen.
    pub bonus: Vec<(u8, u8)>,
    /// Married to (character id, name).
    pub spouse: Option<(i64, String)>,
    /// The player this one asked to marry (TSAO flags.MandoSolicitudA).
    pub proposed_to: Option<u32>,
}

/// World-wide state: the data and the open poll.
#[derive(Default)]
pub struct TsaoWorld {
    data: Option<Arc<TsaoProgress>>,
    /// The poll opened with /encuesta (replaces the one of Consultas.dat), and its key in
    /// the vote table.
    poll: Option<(String, ao_formats::dat::tsao_progress::Poll)>,
    poll_loaded: bool,
}

/// What the chosen bonuses add (tenths of a percent, HP points, stab points).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BonusMods {
    pub hp: i32,
    pub evasion: i32,
    pub shield: i32,
    pub weapon_hit: i32,
    pub weapon_damage: i32,
    pub arrow_hit: i32,
    pub arrow_damage: i32,
    pub magic_resist: i32,
    pub magic_damage: i32,
    pub stab: i32,
}

impl BonusMods {
    /// `value` raised by `tenths` tenths of a percent.
    pub fn scale(value: i64, tenths: i32) -> i64 {
        value + value * tenths as i64 / 1000
    }
}

fn points_view(p: &PointsRow) -> Points {
    Points { tournament: p.tournament, ts: p.ts, credits: p.credits }
}

impl World {
    pub(crate) fn tsao_data(&mut self) -> Arc<TsaoProgress> {
        if self.tsao.data.is_none() {
            self.tsao.data = Some(Arc::new(ao_formats::dat::tsao_progress::load(&self.recursos)));
        }
        self.tsao.data.clone().unwrap()
    }

    fn tsao_poll(&mut self) -> Option<(String, ao_formats::dat::tsao_progress::Poll)> {
        if !self.tsao.poll_loaded {
            self.tsao.poll_loaded = true;
            if let Some(p) = self.tsao_data().poll.clone() {
                self.tsao.poll = Some((format!("consulta:{}", p.question), p));
            }
        }
        self.tsao.poll.clone()
    }

    /// HOOK (session): loads the piece, sends the points and the bonuses, and offers a
    /// bonus the player reached but did not choose.
    pub(crate) fn tsao_on_enter(&mut self, id: u32) {
        // Loaded here so the combat accessor (`tsao_bonus`, &self) always has the data.
        self.tsao_data();
        let Some(char_id) = self.player(id).and_then(|p| p.char_id) else { return };
        let points = self.db.points_load(char_id).unwrap_or_else(|e| {
            warn!(?e, "tsao: no se pudieron leer los puntos");
            PointsRow::default()
        });
        let bonus = self.db.bonus_load(char_id).unwrap_or_default();
        let spouse = self.db.spouse(char_id).unwrap_or_default();
        let p = self.player_mut(id);
        p.tsao = TsaoPiece { points, bonus, spouse, proposed_to: None };
        self.tsao_send_points(id);
        self.tsao_send_bonuses(id);
        self.tsao_offer_bonus(id);
    }

    fn tsao_send_points(&self, id: u32) {
        if let Some(p) = self.player(id) {
            self.send(id, &S2C::Tsao(TsaoS2C::Points(points_view(&p.tsao.points))));
        }
    }

    fn tsao_save_points(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        if let Some(c) = p.char_id {
            if let Err(e) = self.db.points_save(c, &p.tsao.points) {
                warn!(?e, name = %p.name, "tsao: no se pudieron guardar los puntos");
            }
        }
    }

    /// Adds points of each kind (any may be 0), saves and tells the player
    /// (TSAO messages 57, 900 and 930).
    pub(crate) fn tsao_add_points(&mut self, id: u32, tournament: u32, ts: u32, credits: u32) {
        if tournament == 0 && ts == 0 && credits == 0 || self.player(id).is_none() {
            return;
        }
        let pts = &mut self.player_mut(id).tsao.points;
        pts.tournament = pts.tournament.saturating_add(tournament);
        pts.ts = pts.ts.saturating_add(ts);
        pts.credits = pts.credits.saturating_add(credits);
        self.tsao_save_points(id);
        self.tsao_send_points(id);
        if tournament > 0 {
            self.console(id, format!("Has ganado {tournament} puntos de torneo."), Tone::Good);
        }
        if ts > 0 {
            self.console(id, format!("Has ganado {ts} puntos TS."), Tone::Good);
        }
        if credits > 0 {
            self.console(id, format!("Has recibido {credits} créditos de donación."), Tone::Good);
        }
    }

    /// HOOK (combat): NPCDarPuntos, the `GivePTS` of the creature.
    pub(crate) fn tsao_on_npc_killed(&mut self, killer: u32, npc_number: u16) {
        let pts = self.ao.npcs.get(npc_number as u32).map_or(0, |n| n.give_points);
        if pts > 0 {
            self.tsao_add_points(killer, pts as u32, 0, 0);
        }
    }

    /// HOOK (quest): the tournament points, TS points and credits of a completed quest.
    pub(crate) fn tsao_quest_reward(&mut self, id: u32, q: &ao_formats::dat::quests::Quest) {
        self.tsao_add_points(id, q.reward_tournament.max(0) as u32, q.reward_ts.max(0) as u32, q.reward_credits.max(0) as u32);
    }

    /// HOOK (progression): levels `first..=last` were reached: their points (once per
    /// character) and the class bonus offer.
    pub(crate) fn tsao_on_level(&mut self, id: u32, first: u32, last: u32) {
        let data = self.tsao_data();
        for lp in data.level_points.iter().filter(|l| (first..=last).contains(&l.level)) {
            if self.player(id).is_none_or(|p| p.tsao.points.levels_paid.contains(&lp.level)) {
                continue;
            }
            self.player_mut(id).tsao.points.levels_paid.push(lp.level);
            self.tsao_save_points(id);
            if lp.tournament > 0 || lp.ts > 0 {
                let name = self.player(id).unwrap().name.clone();
                self.broadcast_all(&S2C::Console { text: format!("{name} ha alcanzado el nivel {}.", lp.level), color: Tone::Good }, None);
            }
            self.tsao_add_points(id, lp.tournament, lp.ts, 0);
        }
        self.tsao_offer_bonus(id);
    }

    /// The first bonus tier reached and not chosen yet, if any.
    fn tsao_pending_tier(&mut self, id: u32) -> Option<(u8, u32, [String; 2])> {
        let data = self.tsao_data();
        let p = self.player(id)?;
        let class = ao_formats::dat::balance::Class::from_id(p.class as i32)?;
        data.bonus.levels.iter().enumerate().find_map(|(tier, &level)| {
            if p.level < level || p.tsao.bonus.iter().any(|(t, _)| *t as usize == tier) {
                return None;
            }
            let o = data.bonus.options(class.spanish_name(), tier)?;
            Some((tier as u8, level, [o[0].text.clone(), o[1].text.clone()]))
        })
    }

    fn tsao_offer_bonus(&mut self, id: u32) {
        if let Some((tier, level, options)) = self.tsao_pending_tier(id) {
            self.send(id, &S2C::Tsao(TsaoS2C::BonusOffer { tier, level, options }));
        }
    }

    fn tsao_send_bonuses(&mut self, id: u32) {
        let data = self.tsao_data();
        let Some(p) = self.player(id) else { return };
        let Some(class) = ao_formats::dat::balance::Class::from_id(p.class as i32) else { return };
        let texts = p.tsao.bonus.iter().filter_map(|(t, o)| data.bonus.options(class.spanish_name(), *t as usize).map(|op| op[*o as usize & 1].text.clone())).collect();
        self.send(id, &S2C::Tsao(TsaoS2C::Bonuses(texts)));
    }

    /// What the chosen bonuses of player `id` add. For combat and magic.
    pub(crate) fn tsao_bonus(&self, id: u32) -> BonusMods {
        let mut m = BonusMods::default();
        let (Some(p), Some(data)) = (self.player(id), self.tsao.data.as_ref()) else { return m };
        let Some(class) = ao_formats::dat::balance::Class::from_id(p.class as i32) else { return m };
        for (t, o) in &p.tsao.bonus {
            let Some(op) = data.bonus.options(class.spanish_name(), *t as usize).map(|op| &op[*o as usize & 1]) else { continue };
            let slot = match op.effect {
                BonusEffect::Hp => &mut m.hp,
                BonusEffect::Evasion => &mut m.evasion,
                BonusEffect::Shield => &mut m.shield,
                BonusEffect::WeaponHit => &mut m.weapon_hit,
                BonusEffect::WeaponDamage => &mut m.weapon_damage,
                BonusEffect::ArrowHit => &mut m.arrow_hit,
                BonusEffect::ArrowDamage => &mut m.arrow_damage,
                BonusEffect::MagicResist => &mut m.magic_resist,
                BonusEffect::MagicDamage => &mut m.magic_damage,
                BonusEffect::Stab => &mut m.stab,
            };
            *slot += op.amount;
        }
        m
    }

    fn tsao_choose_bonus(&mut self, id: u32, tier: u8, option: u8) {
        let Some((pending, ..)) = self.tsao_pending_tier(id) else { return self.strike(id, "bonus: ninguno pendiente") };
        if pending != tier || option > 1 {
            return self.strike(id, "bonus: nivel u opción falsos");
        }
        // Development logins (no character row) keep it in memory only.
        if let Some(char_id) = self.player(id).and_then(|p| p.char_id) {
            match self.db.bonus_choose(char_id, tier, option) {
                Ok(true) => {}
                Ok(false) => return,
                Err(e) => return warn!(?e, "tsao: no se pudo guardar el bonus"),
            }
        }
        let data = self.tsao_data();
        let class = ao_formats::dat::balance::Class::from_id(self.player(id).unwrap().class as i32).unwrap();
        let op = data.bonus.options(class.spanish_name(), tier as usize).unwrap()[option as usize].clone();
        let p = self.player_mut(id);
        p.tsao.bonus.push((tier, option));
        if op.effect == BonusEffect::Hp {
            p.max_hp += op.amount;
            p.hp = p.max_hp;
            let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
            self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
            self.send_stats(id);
        }
        if let Some(p) = self.player(id) {
            self.save_player(p);
        }
        info!(name = %self.player(id).unwrap().name, tier, option, text = %op.text, "bonus de clase");
        self.console(id, format!("Bonificación elegida: {}", op.text), Tone::Good);
        self.tsao_send_bonuses(id);
        self.tsao_offer_bonus(id);
    }

    // ---- exchange ----

    fn tsao_exchange(&mut self, id: u32) {
        let data = self.tsao_data();
        let Some(p) = self.player(id) else { return };
        let short = matches!(ao_formats::dat::balance::Race::from_id(p.race as i32), Some(ao_formats::dat::balance::Race::Gnome | ao_formats::dat::balance::Race::Dwarf));
        let name = |o: u16| self.ao.objs.get(o as u32).map_or_else(|| format!("objeto {o}"), |d| d.name.clone());
        let prizes = data.prizes.iter().map(|z| PrizeView { obj: z.obj, name: z.name.clone(), price: z.price, stats: [z.hit, z.def, z.magic_hit, z.magic_def], desc: z.desc.clone() }).collect();
        let ts = data.ts_shop.iter().map(|t| TsView { obj: t.obj, amount: t.amount, price: t.price }).collect();
        let packs = data
            .packs
            .iter()
            .map(|k| PackView {
                name: k.name.clone(),
                desc: k.desc.clone(),
                price: k.price,
                body: if short && k.body_short > 0 { k.body_short } else { k.body },
                contents: k
                    .rewards
                    .iter()
                    .map(|r| match r {
                        PackReward::Item { obj, amount } => format!("{amount} {}", name(*obj)),
                        PackReward::TournamentPoints(n) => format!("{n} puntos de torneo"),
                        PackReward::Spell(s) => format!("Hechizo {}", self.ao.spells.get(*s as u32).map_or("?", |s| s.name.as_str())),
                        PackReward::Premium(m) => format!("Pack Premium ({m} mes) - no disponible"),
                        PackReward::Skin { obj, .. } => format!("Skin de {} - no disponible", name(*obj)),
                    })
                    .collect(),
                available: k.price > 0 && !k.rewards.is_empty() && k.rewards.iter().all(PackReward::supported),
            })
            .collect();
        self.send(id, &S2C::Tsao(TsaoS2C::Exchange { prizes, ts, packs }));
        self.tsao_send_points(id);
    }

    /// Common checks; the player may exchange now.
    fn tsao_can_exchange(&self, id: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        if p.trade.with.is_some() || p.trade.shop.is_some() {
            self.console(id, "No puedes canjear mientras comercias.", Tone::Warn);
            return false;
        }
        true
    }

    /// Gives `items` (all must fit: checked before) and logs.
    fn tsao_give(&mut self, id: u32, items: &[(u16, u32)], what: &str) {
        for (obj, amount) in items {
            self.add_item(id, InvItem { obj: *obj, amount: *amount });
            let n = self.ao.objs.get(*obj as u32).map_or(String::new(), |o| o.name.clone());
            self.console(id, format!("Has canjeado {amount} {n}."), Tone::Good);
        }
        if let Some(p) = self.player(id) {
            info!(name = %p.name, ?items, what, "canje");
            self.save_player(p);
        }
        self.tsao_save_points(id);
        self.tsao_send_points(id);
    }

    fn tsao_buy_prize(&mut self, id: u32, index: u16, amount: u32) {
        let data = self.tsao_data();
        let Some(z) = data.prizes.get(index as usize) else { return self.strike(id, "canje: premio falso") };
        if !(1..=MAX_PRIZE_UNITS).contains(&amount) {
            return self.strike(id, "canje: cantidad falsa");
        }
        let cost = z.price as u64 * amount as u64;
        let have = self.player(id).unwrap().tsao.points.tournament as u64;
        if have < cost {
            return self.console(id, format!("Necesitas {cost} puntos de torneo para {amount} {}.", z.name), Tone::Warn);
        }
        if !self.fits_in_bag(id, &[(z.obj, amount)]) {
            return self.console(id, "No tienes espacio en el inventario.", Tone::Warn);
        }
        self.player_mut(id).tsao.points.tournament -= cost as u32;
        self.tsao_give(id, &[(z.obj, amount)], "puntos de torneo");
    }

    fn tsao_buy_ts(&mut self, id: u32, index: u16) {
        let data = self.tsao_data();
        let Some(t) = data.ts_shop.get(index as usize).copied() else { return self.strike(id, "canje: objeto TS falso") };
        if self.player(id).unwrap().tsao.points.ts < t.price {
            return self.console(id, format!("Necesitas {} puntos TS.", t.price), Tone::Warn);
        }
        if !self.fits_in_bag(id, &[(t.obj, t.amount)]) {
            return self.console(id, "No tienes espacio en el inventario.", Tone::Warn);
        }
        self.player_mut(id).tsao.points.ts -= t.price;
        self.tsao_give(id, &[(t.obj, t.amount)], "puntos TS");
    }

    fn tsao_buy_pack(&mut self, id: u32, index: u16) {
        let data = self.tsao_data();
        let Some(k) = data.packs.get(index as usize) else { return self.strike(id, "canje: pack falso") };
        if k.price == 0 || k.rewards.is_empty() || !k.rewards.iter().all(PackReward::supported) {
            return self.console(id, "Ese pack no se puede canjear en este servidor.", Tone::Warn);
        }
        if self.player(id).unwrap().tsao.points.credits < k.price {
            return self.console(id, format!("Necesitas {} créditos de donación.", k.price), Tone::Warn);
        }
        let items: Vec<(u16, u32)> = k.rewards.iter().filter_map(|r| if let PackReward::Item { obj, amount } = r { Some((*obj, *amount)) } else { None }).collect();
        if !self.fits_in_bag(id, &items) {
            return self.console(id, "No tienes espacio en el inventario para todo el pack.", Tone::Warn);
        }
        let spells: Vec<u16> = k.rewards.iter().filter_map(|r| if let PackReward::Spell(s) = r { Some(*s) } else { None }).filter(|s| self.ao.spells.get(*s as u32).is_some()).collect();
        let new_spells = spells.iter().filter(|s| !self.player(id).unwrap().spells.knows(**s)).count();
        if new_spells > self.player(id).unwrap().spells.slots.iter().filter(|s| **s == 0).count() {
            return self.console(id, "No tienes espacio para más hechizos.", Tone::Warn);
        }
        let points: u32 = k.rewards.iter().map(|r| if let PackReward::TournamentPoints(n) = r { *n } else { 0 }).sum();
        let p = self.player_mut(id);
        p.tsao.points.credits -= k.price;
        p.tsao.points.tournament = p.tsao.points.tournament.saturating_add(points);
        for s in spells {
            if !self.player(id).unwrap().spells.knows(s) && self.player_mut(id).spells.add(s).is_some() {
                let n = self.ao.spells.get(s as u32).unwrap().name.clone();
                self.console(id, format!("Has aprendido el hechizo {n}."), Tone::Good);
                self.send_spellbook(id);
            }
        }
        if points > 0 {
            self.console(id, format!("Has ganado {points} puntos de torneo."), Tone::Good);
        }
        let name = k.name.clone();
        self.tsao_give(id, &items, &name);
        self.console(id, format!("Has canjeado {name}."), Tone::Good);
    }

    pub(crate) fn on_tsao(&mut self, id: u32, m: TsaoC2S) {
        match m {
            TsaoC2S::OpenExchange => {
                if self.tsao_can_exchange(id) {
                    self.tsao_exchange(id)
                }
            }
            TsaoC2S::BuyPrize { index, amount } => {
                if self.tsao_can_exchange(id) {
                    self.tsao_buy_prize(id, index, amount)
                }
            }
            TsaoC2S::BuyTs { index } => {
                if self.tsao_can_exchange(id) {
                    self.tsao_buy_ts(id, index)
                }
            }
            TsaoC2S::BuyPack { index } => {
                if self.tsao_can_exchange(id) {
                    self.tsao_buy_pack(id, index)
                }
            }
            TsaoC2S::ChooseBonus { tier, option } => self.tsao_choose_bonus(id, tier, option),
            TsaoC2S::Vote { option } => self.tsao_vote(id, option),
            TsaoC2S::Upgrade { index } => {
                if self.tsao_can_exchange(id) {
                    self.tsao_upgrade(id, index)
                }
            }
        }
    }

    // ---- upgrades (Mejorados.dat) ----

    /// First bag slot holding `obj`, not equipped.
    fn tsao_free_slot_of(&self, id: u32, obj: u16) -> Option<u8> {
        let p = self.player(id)?;
        (1..=p.inv.usable_slots()).find(|s| p.inv.get(*s).is_some_and(|i| i.obj == obj) && !p.inv.equipped.is_equipped(*s) && !p.inv.pending_equipped.contains(s))
    }

    fn tsao_gem_slot(&self, id: u32) -> Option<u8> {
        let p = self.player(id)?;
        (1..=p.inv.usable_slots()).find(|s| p.inv.get(*s).and_then(|i| self.ao.objs.get(i.obj as u32)).is_some_and(|o| o.tsao_type == TSAO_OCTARINE_GEM))
    }

    /// HOOK (inventory): the Octarine gem was used: the upgrades of what the bag holds.
    pub(crate) fn tsao_upgrades(&mut self, id: u32) {
        let data = self.tsao_data();
        let list: Vec<_> = data
            .upgrades
            .iter()
            .enumerate()
            .filter(|(_, u)| self.tsao_free_slot_of(id, u.requires).is_some())
            .map(|(i, u)| {
                let z = &u.result;
                (i as u16, u.requires, PrizeView { obj: z.obj, name: z.name.clone(), price: 0, stats: [z.hit, z.def, z.magic_hit, z.magic_def], desc: z.desc.clone() })
            })
            .collect();
        if list.is_empty() {
            self.console(id, "No tienes objetos que se puedan mejorar (desequípalos primero).", Tone::Info);
        }
        self.send(id, &S2C::Tsao(TsaoS2C::Upgrades(list)));
    }

    fn tsao_upgrade(&mut self, id: u32, index: u16) {
        let data = self.tsao_data();
        let Some(up) = data.upgrades.get(index as usize) else { return self.strike(id, "mejorar: indice falso") };
        let Some(gem) = self.tsao_gem_slot(id) else { return self.console(id, "Necesitas una Gema Octarina.", Tone::Warn) };
        let Some(slot) = self.tsao_free_slot_of(id, up.requires) else { return self.console(id, "No tienes ese objeto (o lo llevas equipado).", Tone::Warn) };
        // Room: the two go out first, so a full bag still fits the result when the
        // required object was alone in its slot; check against the bag without them.
        let frees = self.player(id).unwrap().inv.get(slot).is_some_and(|i| i.amount == 1) || self.player(id).unwrap().inv.get(gem).is_some_and(|i| i.amount == 1);
        if !frees && !self.fits_in_bag(id, &[(up.result.obj, 1)]) {
            return self.console(id, "No tienes espacio en el inventario.", Tone::Warn);
        }
        let from = self.ao.objs.get(up.requires as u32).map_or(String::new(), |o| o.name.clone());
        self.remove_from_slot(id, slot, 1);
        self.remove_from_slot(id, gem, 1);
        self.add_item(id, InvItem { obj: up.result.obj, amount: 1 });
        self.send_inventory_vitals(id);
        if let Some(p) = self.player(id) {
            info!(name = %p.name, from = up.requires, to = up.result.obj, "mejora de objeto");
            self.save_player(p);
        }
        self.console(id, format!("Has mejorado {from}: ahora es {}.", up.result.name), Tone::Good);
        self.tsao_upgrades(id);
    }

    // ---- poll ----

    fn tsao_poll_window(&mut self, id: u32) {
        let Some((key, poll)) = self.tsao_poll() else { return self.console(id, "No hay ninguna votación abierta.", Tone::Info) };
        let acc = self.player(id).and_then(|p| p.char_id).and_then(|c| self.db.account_of(c).ok().flatten());
        let results = if acc.is_some_and(|acc| self.db.poll_voted(&key, acc).unwrap_or(false)) { self.db.poll_results(&key, poll.options.len()).ok() } else { None };
        self.send(id, &S2C::Tsao(TsaoS2C::Poll { question: poll.question, options: poll.options, results }));
    }

    fn tsao_vote(&mut self, id: u32, option: u8) {
        let Some((key, poll)) = self.tsao_poll() else { return self.console(id, "No hay ninguna votación abierta.", Tone::Info) };
        if option as usize >= poll.options.len() {
            return self.strike(id, "votar: opción falsa");
        }
        let Some(p) = self.player(id) else { return };
        if p.level < poll.level {
            return self.console(id, format!("Para votar debes ser nivel {} o superior.", poll.level), Tone::Warn);
        }
        let Some(acc) = p.char_id.and_then(|c| self.db.account_of(c).ok().flatten()) else { return self.console(id, "Solo los personajes de una cuenta pueden votar.", Tone::Warn) };
        match self.db.poll_vote(&key, acc, option) {
            Ok(true) => self.console(id, format!("Tu voto ha sido computado: {}.", poll.options[option as usize]), Tone::Good),
            Ok(false) => self.console(id, "Tu cuenta ya votó en esta consulta.", Tone::Warn),
            Err(e) => return warn!(?e, "votar"),
        }
        self.tsao_poll_window(id);
    }

    // ---- marriage ----

    fn tsao_marry(&mut self, id: u32, target: &str) {
        let Some(t) = self.id_by_name(target) else { return self.console(id, "El usuario no está conectado.", Tone::Warn) };
        if t == id {
            return;
        }
        let (me, other) = (self.player(id).unwrap(), self.player(t).unwrap());
        let (Some(a), Some(b)) = (me.char_id, other.char_id) else { return };
        if me.tsao.spouse.is_some() {
            return self.console(id, "Ya estás casado/a.", Tone::Warn);
        }
        if other.tsao.spouse.is_some() {
            return self.console(id, "Esa persona ya está casada.", Tone::Warn);
        }
        let (my_name, other_name) = (me.name.clone(), other.name.clone());
        if other.tsao.proposed_to != Some(id) {
            self.player_mut(id).tsao.proposed_to = Some(t);
            self.console(id, format!("Le has pedido matrimonio a {other_name}."), Tone::Info);
            return self.console(t, format!("{my_name} te ha pedido matrimonio. Escribe /casar {my_name} para aceptar."), Tone::Good);
        }
        match self.db.marry(a, b) {
            Ok(true) => {}
            Ok(false) => return self.console(id, "Uno de los dos ya está casado.", Tone::Warn),
            Err(e) => return warn!(?e, "casar"),
        }
        self.player_mut(id).tsao = TsaoPiece { spouse: Some((b, other_name.clone())), proposed_to: None, ..self.player(id).unwrap().tsao.clone() };
        self.player_mut(t).tsao = TsaoPiece { spouse: Some((a, my_name.clone())), proposed_to: None, ..self.player(t).unwrap().tsao.clone() };
        self.broadcast_all(&S2C::Console { text: format!("¡{my_name} y {other_name} se han casado!"), color: Tone::Good }, None);
        // TCP_HandleData3.bas: FX 23 on both.
        for who in [id, t] {
            let map = self.player(who).unwrap().map;
            self.broadcast(map, &S2C::Fx { id: who, fx: 23, loops: 0 }, None);
        }
    }

    fn tsao_divorce(&mut self, id: u32) {
        let Some(a) = self.player(id).and_then(|p| p.char_id) else { return };
        let Some((_, name)) = self.player(id).unwrap().tsao.spouse.clone() else { return self.console(id, "No estás casado/a.", Tone::Warn) };
        if let Err(e) = self.db.divorce(a) {
            return warn!(?e, "divorcio");
        }
        self.player_mut(id).tsao.spouse = None;
        if let Some(t) = self.id_by_name(&name) {
            self.player_mut(t).tsao.spouse = None;
        }
        let me = self.player(id).unwrap().name.clone();
        self.broadcast_all(&S2C::Console { text: format!("{me} y {name} se han divorciado."), color: Tone::Info }, None);
    }

    /// HOOK (interact): clicking a married player shows "Nombre <Matrimonio con X>"
    /// (GameLogic.bas LookatTile). False when there is no married player there.
    pub(crate) fn tsao_click_player(&self, id: u32, map: u16, tx: i32, ty: i32) -> bool {
        let Some(t) = self.player_ids().into_iter().find(|t| self.player(*t).is_some_and(|p| p.map == map && p.x == tx && p.y == ty)) else { return false };
        let p = self.player(t).unwrap();
        let Some((_, spouse)) = &p.tsao.spouse else { return false };
        self.console(id, format!("{} <Matrimonio con {spouse}>", p.name), Tone::Info);
        true
    }

    /// Player commands of this module; true if the line was one.
    /// `/canje`, `/votar`, `/resultados`, `/casar <nick>`, `/divorciarse`, `/puntos`,
    /// `/bonus`; GMs: `/encuesta pregunta@op1@op2[@op3..]@nivel`; dev: `/darpuntos t ts c`.
    pub(crate) fn tsao_command(&mut self, id: u32, line: &str) -> bool {
        let (cmd, rest) = line.split_once(' ').map_or((line, ""), |(c, r)| (c, r.trim()));
        match cmd.to_lowercase().as_str() {
            "/canje" => {
                if self.tsao_can_exchange(id) {
                    self.tsao_exchange(id);
                }
            }
            "/votar" | "/resultados" => self.tsao_poll_window(id),
            // Numic: the same as using the gem from the bag.
            "/mejorar" => {
                if self.tsao_gem_slot(id).is_some() {
                    self.tsao_upgrades(id);
                } else {
                    self.console(id, "Necesitas una Gema Octarina.", Tone::Warn);
                }
            }
            "/casar" if !rest.is_empty() => self.tsao_marry(id, rest),
            "/divorciarse" => self.tsao_divorce(id),
            "/puntos" | "/est" => {
                let Some(p) = self.player(id) else { return true };
                let t = &p.tsao.points;
                let mut text = format!("Puntos de torneo: {} · Puntos TS: {} · Créditos: {}", t.tournament, t.ts, t.credits);
                if let Some((_, s)) = &p.tsao.spouse {
                    text.push_str(&format!(" · Casado/a con {s}"));
                }
                self.console(id, text, Tone::Info);
                self.tsao_send_bonuses(id);
            }
            "/bonus" => {
                if self.tsao_pending_tier(id).is_none() {
                    self.console(id, "No tienes bonificaciones por elegir.", Tone::Info);
                }
                self.tsao_offer_bonus(id);
            }
            "/encuesta" if self.gm_rank(id) > GmRank::User => {
                let parts: Vec<&str> = rest.split('@').map(str::trim).collect();
                let level = parts.last().and_then(|l| l.parse().ok());
                let opts: Vec<String> = parts[1..parts.len() - level.map_or(0, |_| 1)].iter().filter(|o| !o.is_empty() && !o.eq_ignore_ascii_case("n/a")).map(|o| o.to_string()).collect();
                if parts.len() < 3 || opts.len() < 2 || opts.len() > 5 || parts[0].is_empty() {
                    self.console(id, "Uso: /encuesta pregunta@opción1@opción2[@...opción5][@nivel]", Tone::Warn);
                    return true;
                }
                let poll = ao_formats::dat::tsao_progress::Poll { question: parts[0].to_string(), level: level.unwrap_or(1).max(1), options: opts };
                let key = format!("gm:{}:{}", crate::db::now_unix(), poll.question);
                self.tsao.poll_loaded = true;
                self.tsao.poll = Some((key, poll.clone()));
                self.broadcast_all(&S2C::Console { text: format!("Nueva votación: {}. Escribe /votar para participar.", poll.question), color: Tone::Good }, None);
            }
            "/darpuntos" if self.cfg.dev_login => {
                let n: Vec<u32> = rest.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                if let [t, ts, c] = n[..] {
                    self.tsao_add_points(id, t, ts, c);
                }
            }
            // Development: experience through the real level-up path.
            "/exp" if self.cfg.dev_login => {
                if let Ok(n) = rest.parse::<u32>() {
                    self.give_exp(id, n);
                }
            }
            _ => return false,
        }
        true
    }
}
