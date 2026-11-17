//! Craft system: crafting: blacksmithing, carpentry, tailoring, alchemy. OWNER: Opus B (docs/P4_CONTRATO.md).
//!
//! Ported from AO20 (`Codigo/`):
//! - Opening: `WorldActions.HandleAnvilAction` (hammer equipped, anvil within 2 tiles, then
//!   the three blacksmith lists and `ShowBlacksmithForm`) and the working-tool branch of
//!   `InvUsuario.UseInvItem` (pot 4 -> alchemy, saw 5 -> carpentry, sewing box 9 ->
//!   tailoring, hammer 7 -> "click the anvil"; tired players cannot).
//! - Lists: `Protocol_Writes.WriteBlacksmithWeapons/Armors/ElementalRunes`,
//!   `WriteCarpenterObjects`, `WriteAlquimistaObjects`, `WriteSastreObjects`.
//! - Building: `Trabajo.HerreroConstruirItem`, `CarpinteroConstruirItem` (with the amount
//!   of `HandleCraftCarpenter` and the batch of `Trabajar`), `AlquimistaConstruirItem`,
//!   `SastreConstruirItem`, with `*TieneMateriales`, `*QuitarMateriales`,
//!   `KnowsCraftingRecipe`, `IntervaloPermiteTrabajarConstruir` and `SubirSkill`.
//!
//! Differences, on purpose: every trade takes an amount (AO20: only the carpenter; the
//! client macro repeats the rest), the order runs on the server one step per interval and
//! every step re-checks tool, anvil distance, skill, recipe, stamina and materials; the
//! window list and the build use the same predicate (`ao_rules::craft::can_build`).
use crate::clock::GameTime;
use crate::world::craft::{Job, OpenTrade};
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::balance::Class;
use ao_formats::dat::craft::{CraftLists, MapObject};
use ao_formats::dat::obj::ObjType;
use ao_protocol::p4::craft::Recipe;
use ao_protocol::{CraftC2S, CraftS2C, Tone, S2C};
use ao_rules::craft::{self as rules, Trade};
use ao_rules::items::MAX_STACK;
use std::sync::Arc;

const DEAD: &str = "¡¡Estás muerto!!";
const TIRED: &str = "Estás muy cansado.";
const TOO_FAR: &str = "Estás muy lejos.";
const EQUIP_FIRST: &str = "Antes de usar la herramienta deberías equipártela.";
const NEED_HAMMER: &str = "Debes tener equipado un martillo de herrero para trabajar con el yunque.";
const WRONG_HAMMER: &str = "La herramienta que tienes no es la correcta, necesitas un martillo de herrero para poder trabajar.";
const CLICK_ANVIL: &str = "Debes hacer click derecho sobre el yunque.";
const NO_ROOM: &str = "No tienes suficiente espacio en el inventario.";
const NO_RECIPE: &str = "Lamentablemente no aprendiste la receta para crear este objeto.";
const BLODIUM_ANVIL: &str = "Necesitás un yunque de blodium para trabajar el blodium.";

fn send_craft(w: &World, id: u32, m: CraftS2C) {
    w.send(id, &S2C::Craft(m));
}

impl World {
    pub(crate) fn on_craft(&mut self, id: u32, msg: CraftC2S) {
        match msg {
            CraftC2S::Open { trade, station } => self.craft_open(id, trade, station),
            CraftC2S::Build { obj, amount } => self.craft_build(id, obj, amount),
            CraftC2S::Stop => {
                if self.player_mut(id).craft.job.take().is_some() {
                    send_craft(self, id, CraftS2C::Stopped { reason: "Dejaste de trabajar.".into() });
                }
            }
            CraftC2S::Close => self.craft_close(id, false),
        }
    }

    /// HOOK (interact): the player talked to a crafting NPC (`Crafts > 0` in npcs.dat),
    /// alive and near. That is `modCrafteos` (combining up to 5 items with a catalyst),
    /// and no NPC of the official npcs.dat sets `Craftea`: not ported.
    pub(crate) fn craft_npc(&mut self, id: u32, npc: u32) {
        let _ = npc;
        self.console(id, "Este artesano no tiene nada para ofrecerte.", Tone::Info);
    }

    /// For the integrator (inventory "use"): the player used a working tool from `slot`.
    /// Returns true when the tool belongs to a craft trade (then nothing else should
    /// happen): pot, saw and sewing box open their trade, the hammer says to click the
    /// anvil (`UseInvItem`, `otWorkingTools`).
    #[allow(dead_code)]
    pub(crate) fn craft_use_tool(&mut self, id: u32, slot: u8) -> bool {
        let ao = self.ao.clone();
        let Some(o) = self.player(id).and_then(|p| p.inv.get(slot)).and_then(|i| ao.objs.get(i.obj as u32)) else { return false };
        if o.obj_type != ObjType::WorkingTools {
            return false;
        }
        match Trade::ALL.into_iter().find(|t| t.tool_subtype() == o.subtype) {
            Some(Trade::Blacksmith) => {
                self.console(id, CLICK_ANVIL, Tone::Warn);
                true
            }
            Some(t) => {
                self.craft_open(id, t, None);
                true
            }
            None => false,
        }
    }

    /// For the integrator (world click): the player clicked tile (x, y). Returns true if
    /// there was an anvil (`TryHandleObjectWorldAction`, `otAnvil`) and it was handled.
    #[allow(dead_code)]
    pub(crate) fn craft_click_tile(&mut self, id: u32, x: i32, y: i32) -> bool {
        let Some(map) = self.player(id).map(|p| p.map) else { return false };
        if self.craft_anvil_at(map, x, y).is_none() {
            return false;
        }
        self.craft_open(id, Trade::Blacksmith, Some((x, y)));
        true
    }

    /// Every world tick: runs the due build steps and closes the anvil of whoever walked
    /// away from it.
    pub(crate) fn tick_craft(&mut self, now: GameTime) {
        if self.crafts.active.is_empty() {
            return;
        }
        let ids: Vec<u32> = self.crafts.active.iter().copied().collect();
        for id in ids {
            let Some(p) = self.player(id) else {
                self.crafts.active.remove(&id);
                continue;
            };
            let Some(open) = p.craft.open else {
                self.crafts.active.remove(&id);
                continue;
            };
            if let Some((map, x, y, _)) = open.station {
                if p.map != map || rules::distance((p.x, p.y), (x, y)) > rules::ANVIL_RANGE {
                    self.console(id, TOO_FAR, Tone::Info);
                    self.craft_close(id, true);
                    continue;
                }
            }
            if p.craft.job.is_some_and(|j| j.next_at <= now) {
                self.craft_step(id);
            }
        }
    }

    /// When the player enters the world (after everything else is sent).
    pub(crate) fn craft_on_enter(&mut self, id: u32) {
        self.craft_dev_kit(id);
    }

    /// Screenshots and manual tests only: with `dev_login` and `AO_CRAFT_DEV=hammer|saw|
    /// pot|box` on the server, a dev character becomes a worker with trade skills, every
    /// tool (that one equipped), materials and the potion recipes.
    fn craft_dev_kit(&mut self, id: u32) {
        let Ok(tool) = std::env::var("AO_CRAFT_DEV") else { return };
        if !self.cfg.dev_login || self.player(id).is_none_or(|p| p.char_id.is_some()) {
            return;
        }
        {
            let p = self.player_mut(id);
            p.class = Class::Worker as u8;
            p.level = p.level.max(40);
            for (t, v) in [(Trade::Blacksmith, 70), (Trade::Carpenter, 60), (Trade::Alchemy, 45), (Trade::Tailor, 40)] {
                p.stats.skills[t.skill()] = v;
            }
            p.spells.add(285);
            p.spells.add(286);
        }
        let kit: [(u16, u32); 11] = [(390, 1), (198, 1), (887, 1), (886, 1), (rules::IRON_INGOT, 400), (rules::SILVER_INGOT, 40), (rules::WOOD, 2500), (rules::PELT_WOLF, 120), (rules::RED_FLOWER, 8), (rules::ALCHEMY_FLASK, 8), (rules::MORTAR, 8)];
        for (obj, amount) in kit {
            self.add_item(id, InvItem { obj, amount });
        }
        let want = match tool.as_str() {
            "saw" => 198,
            "pot" => 887,
            "box" => 886,
            _ => 390,
        };
        let slot = self.player(id).and_then(|p| (1..=p.inv.usable_slots()).find(|s| p.inv.get(*s).is_some_and(|i| i.obj == want)));
        if let Some(s) = slot {
            self.equip(id, s, false);
        }
        self.send_skills(id);
        self.send_spellbook(id);
        self.send_stats(id);
    }

    /// Right before the player leaves the world, still present: the order is dropped
    /// (nothing half-built is kept).
    pub(crate) fn craft_on_leave(&mut self, id: u32) {
        if let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) {
            p.craft = Default::default();
        }
        self.crafts.active.remove(&id);
    }

    // ------------------------------------------------------------ data

    fn craft_lists(&mut self) -> Arc<CraftLists> {
        if let Some(l) = &self.crafts.lists {
            return l.clone();
        }
        let loaded = ao_formats::dat::craft::load(&self.recursos);
        for w in &loaded.warnings {
            tracing::warn!(?w, "dato oficial (artesania)");
        }
        let l = Arc::new(loaded.data);
        self.crafts.lists = Some(l.clone());
        l
    }

    fn craft_map_objects(&mut self, map: u16) -> Arc<Vec<MapObject>> {
        let recursos = self.recursos.clone();
        self.crafts.map_objects.entry(map).or_insert_with(|| Arc::new(ao_formats::dat::craft::map_objects(&recursos, map))).clone()
    }

    /// `Subtipo` of the anvil lying on (x, y) of `map`, if any.
    pub(crate) fn craft_anvil_at(&mut self, map: u16, x: i32, y: i32) -> Option<i32> {
        let ao = self.ao.clone();
        self.craft_map_objects(map).iter().find(|o| o.x == x && o.y == y).and_then(|o| ao.objs.get(o.obj as u32)).filter(|d| d.obj_type == ObjType::Anvil).map(|d| d.subtype)
    }

    /// The `Subtipo` of the equipped working tool (None: nothing equipped).
    fn craft_tool(&self, id: u32) -> Option<i32> {
        let p = self.player(id)?;
        let item = p.inv.get(p.inv.equipped.tool?)?;
        let o = self.ao.objs.get(item.obj as u32)?;
        (o.obj_type == ObjType::WorkingTools).then_some(o.subtype)
    }

    fn craft_class(&self, id: u32) -> Option<Class> {
        self.player(id).and_then(|p| Class::from_id(p.class as i32))
    }

    /// What the player can build in a trade right now (the official list for their skill).
    fn craft_recipes(&mut self, id: u32, trade: Trade) -> Vec<Recipe> {
        let lists = self.craft_lists();
        let ao = self.ao.clone();
        let (skill, class) = (self.player(id).map_or(0, |p| p.stats.skills[trade.skill()]), self.craft_class(id));
        rules::trade_list(trade, &lists)
            .into_iter()
            .filter_map(|obj| {
                let o = ao.objs.get(obj as u32)?;
                rules::can_build(trade, &lists, obj, o, skill, class).then(|| Recipe {
                    obj,
                    skill: trade.required_skill(o).clamp(0, 100) as u8,
                    materials: rules::materials(trade, o, 1).into_iter().map(|m| (m.obj, m.amount)).collect(),
                })
            })
            .collect()
    }

    // ------------------------------------------------------------ inventory helpers

    /// `TieneObjetos`: units of `obj` across the usable slots.
    fn craft_count(&self, id: u32, obj: u16) -> u32 {
        let Some(p) = self.player(id) else { return 0 };
        (1..=p.inv.usable_slots()).filter_map(|s| p.inv.get(s)).filter(|i| i.obj == obj).map(|i| i.amount).sum()
    }

    /// `QuitarObjetos`: takes `amount` units of `obj`, slot by slot.
    fn craft_take(&mut self, id: u32, obj: u16, mut amount: u32) {
        let slots = self.player(id).map_or(0, |p| p.inv.usable_slots());
        for s in 1..=slots {
            if amount == 0 {
                break;
            }
            let Some(item) = self.player(id).and_then(|p| p.inv.get(s)) else { continue };
            if item.obj != obj {
                continue;
            }
            let n = item.amount.min(amount);
            self.remove_from_slot(id, s, n);
            amount -= n;
        }
    }

    /// `HayLugarEnInventario`: whether `amount` of `obj` fit in one slot.
    fn craft_has_room(&self, id: u32, obj: u16, amount: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        (1..=p.inv.usable_slots()).any(|s| match p.inv.get(s) {
            None => true,
            Some(i) => i.obj == obj && i.amount + amount <= MAX_STACK,
        })
    }

    // ------------------------------------------------------------ open / close

    fn craft_open(&mut self, id: u32, trade: Trade, station: Option<(i32, i32)>) {
        let Some(p) = self.player(id) else { return };
        let (map, px, py, dead, sta) = (p.map, p.x, p.y, p.hp <= 0, p.stats.sta);
        if dead {
            return self.console(id, DEAD, Tone::Warn);
        }
        // UseInvItem: "Not MinSta > 0" before any tool is used.
        if sta <= 0 {
            return self.console(id, TIRED, Tone::Info);
        }
        let tool = self.craft_tool(id);
        let mut st = None;
        if trade == Trade::Blacksmith {
            // HandleAnvilAction: distance first, then the hammer.
            let Some((x, y)) = station else { return self.console(id, CLICK_ANVIL, Tone::Warn) };
            let Some(sub) = self.craft_anvil_at(map, x, y) else {
                return self.console(id, "No hay ningún yunque ahí.", Tone::Info);
            };
            if rules::distance((px, py), (x, y)) > rules::ANVIL_RANGE {
                return self.console(id, TOO_FAR, Tone::Info);
            }
            match tool {
                None => return self.console(id, NEED_HAMMER, Tone::Warn),
                Some(t) if t != trade.tool_subtype() => return self.console(id, WRONG_HAMMER, Tone::Warn),
                _ => {}
            }
            st = Some((map, x, y, sub));
        } else if tool != Some(trade.tool_subtype()) {
            return self.console(id, EQUIP_FIRST, Tone::Warn);
        }
        let recipes = self.craft_recipes(id, trade);
        let (skill, class) = (self.player(id).map_or(0, |p| p.stats.skills[trade.skill()]), self.craft_class(id));
        let per_step = rules::units_per_step(trade, class, skill);
        let p = self.player_mut(id);
        let prev = p.craft.open.replace(OpenTrade { trade, station: st });
        if prev.is_some_and(|o| o.trade != trade) && p.craft.job.take().is_some() {
            send_craft(self, id, CraftS2C::Stopped { reason: "Cambiaste de oficio.".into() });
        }
        self.crafts.active.insert(id);
        send_craft(self, id, CraftS2C::Open { trade, recipes, per_step, station: st.map(|(_, x, y, _)| (x, y)) });
    }

    /// Closes the trade (window X, walked away). `notify`: tell the client to close it.
    fn craft_close(&mut self, id: u32, notify: bool) {
        if self.player(id).is_none() {
            return;
        }
        let p = self.player_mut(id);
        let had_job = p.craft.job.take().is_some();
        let had = p.craft.open.take().is_some();
        self.crafts.active.remove(&id);
        if had_job {
            send_craft(self, id, CraftS2C::Stopped { reason: "Dejaste de trabajar.".into() });
        }
        if had && notify {
            send_craft(self, id, CraftS2C::Closed);
        }
    }

    // ------------------------------------------------------------ build

    fn craft_build(&mut self, id: u32, obj: u16, amount: u32) {
        // HandleCraftCarpenter: "Cantidad > 0"; the window never asks for more than a stack.
        if amount == 0 || amount > rules::MAX_ORDER {
            return self.strike(id, "artesania: cantidad invalida");
        }
        let Some(open) = self.player(id).and_then(|p| p.craft.open) else {
            return self.console(id, "No estás trabajando en ningún oficio.", Tone::Info);
        };
        let lists = self.craft_lists();
        let ao = self.ao.clone();
        let Some(o) = ao.objs.get(obj as u32) else { return self.strike(id, "artesania: objeto inexistente") };
        // PuedeConstruirHerreria / Carpintero / Alquimista / Sastre: the window only offers
        // objects of the trade's list.
        if !rules::in_trade_list(open.trade, &lists, obj, o) {
            return self.strike(id, "artesania: objeto fuera de la lista");
        }
        let now = self.now();
        let p = self.player_mut(id);
        p.craft.job = Some(Job { obj, total: amount, done: 0, next_at: now });
        self.craft_step(id);
    }

    /// Ends the order with a message (`WriteMacroTrabajoToggle False` + the locale message).
    fn craft_fail(&mut self, id: u32, msg: &str) {
        self.console(id, msg.to_string(), Tone::Warn);
        self.player_mut(id).craft.job = None;
        send_craft(self, id, CraftS2C::Stopped { reason: msg.to_string() });
    }

    /// One step of the order: `*ConstruirItem` once (the carpenter: a batch).
    fn craft_step(&mut self, id: u32) {
        let now = self.now();
        let Some(p) = self.player(id) else { return };
        let (Some(open), Some(job)) = (p.craft.open, p.craft.job) else { return };
        // IntervaloPermiteTrabajarConstruir.
        if job.next_at > now {
            return;
        }
        let trade = open.trade;
        let (dead, sta, skill, level, food, water) = (p.hp <= 0, p.stats.sta, p.stats.skills[trade.skill()], p.level, p.stats.food, p.stats.water);
        let (map, px, py) = (p.map, p.x, p.y);
        if dead {
            return self.craft_fail(id, DEAD);
        }
        // The tool must still be in the hand (all four check it before building).
        if self.craft_tool(id) != Some(trade.tool_subtype()) {
            return self.craft_fail(id, if trade == Trade::Blacksmith { NEED_HAMMER } else { EQUIP_FIRST });
        }
        if let Some((m, x, y, _)) = open.station {
            if m != map || rules::distance((px, py), (x, y)) > rules::ANVIL_RANGE {
                self.craft_fail(id, TOO_FAR);
                return self.craft_close(id, true);
            }
        }
        let lists = self.craft_lists();
        let ao = self.ao.clone();
        let Some(o) = ao.objs.get(job.obj as u32) else { return self.craft_fail(id, "Ese objeto no existe.") };
        let class = self.craft_class(id);
        if !rules::can_build(trade, &lists, job.obj, o, skill, class) {
            return self.craft_fail(id, "No tenés los conocimientos suficientes para construir eso.");
        }
        // KnowsCraftingRecipe: objects with a `Hechizo` need that spell learnt.
        if o.spell > 0 && !self.player(id).is_some_and(|p| p.spells.knows(o.spell as u16)) {
            return self.craft_fail(id, NO_RECIPE);
        }
        let (min_sta, cost) = trade.stamina();
        if sta <= min_sta {
            self.send(id, &S2C::NpcSay { id, text: "¡No tengo energía!".into() });
            return self.craft_fail(id, TIRED);
        }
        let n = (job.total - job.done).min(rules::units_per_step(trade, class, skill));
        // *TieneMateriales, in the official order, with its message.
        for m in rules::materials(trade, o, n) {
            if self.craft_count(id, m.obj) < m.amount {
                return self.craft_fail(id, m.missing);
            }
        }
        // HerreroTieneMateriales: blodium only on a blodium anvil.
        if trade == Trade::Blacksmith && o.blodium > 0 && open.station.is_none_or(|s| s.3 != rules::BLODIUM_ANVIL) {
            return self.craft_fail(id, BLODIUM_ANVIL);
        }
        // HerreroConstruirItem checks the room first; the other trades drop it to the floor.
        if trade == Trade::Blacksmith && !self.craft_has_room(id, job.obj, n) {
            return self.craft_fail(id, NO_ROOM);
        }

        for m in rules::materials(trade, o, n) {
            self.craft_take(id, m.obj, m.amount);
        }
        {
            let p = self.player_mut(id);
            p.stats.sta = (p.stats.sta - cost).max(0);
        }
        self.send_inventory_vitals(id);
        let item = InvItem { obj: job.obj, amount: n };
        if !self.craft_has_room(id, job.obj, n) || !self.add_item(id, item) {
            // MeterItemEnInventario failed: TirarItemAlPiso.
            self.drop_to_floor(map, px, py, item, false);
        }
        send_craft(self, id, CraftS2C::Crafted { obj: job.obj, amount: n, sound: trade.sound() });
        self.craft_raise_skill(id, trade, skill, level, food, water);

        let done = job.done + n;
        let p = self.player_mut(id);
        p.craft.job = (done < job.total).then_some(Job { done, next_at: now.after_ms(rules::BUILD_INTERVAL_MS), ..job });
        send_craft(self, id, CraftS2C::Progress { obj: job.obj, done, total: job.total });
    }

    /// `SubirSkill` for the trade's skill (there is no raise-by-use elsewhere yet): within
    /// the level cap and fed, `RandomNumber(1, Prob * DificultadSubirSkill) < 10` raises it
    /// by one, says so and gives `5 * ExpMult` experience.
    fn craft_raise_skill(&mut self, id: u32, trade: Trade, skill: u8, level: u32, food: i32, water: i32) {
        if !rules::may_raise(skill, level, food, water) {
            return;
        }
        let max = rules::raise_roll_max(level, self.ao.balance.extra.skill_up_difficulty);
        let roll = 1 + ((self.rng.next_u32() as u64 * max as u64) >> 32) as i32;
        if roll >= rules::SKILL_CUTOFF {
            return;
        }
        let new = skill + 1;
        self.player_mut(id).stats.skills[trade.skill()] = new;
        self.console(id, format!("¡Has mejorado tu habilidad {} en un punto! Ahora tienes {new} pts.", trade.skill_name()), Tone::Good);
        self.send_skills(id);
        self.give_exp(id, rules::SKILL_RAISE_EXP);
        self.send_stats(id);
        // A higher skill may unlock objects: refresh the window list.
        let recipes = self.craft_recipes(id, trade);
        let per_step = rules::units_per_step(trade, self.craft_class(id), new);
        let station = self.player(id).and_then(|p| p.craft.open).and_then(|o| o.station).map(|(_, x, y, _)| (x, y));
        send_craft(self, id, CraftS2C::Open { trade, recipes, per_step, station });
    }
}
