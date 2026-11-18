//! Pets of Tierras Sagradas: tamed creatures, summons, companions and the trainer's
//! creatures. Ported from:
//! - Trabajo.bas `DoDomar` / `CalcularPoderDomador` / `ModDomar`, TCP_HandleData1.bas
//!   `Case Domar` (the click and its checks);
//! - modHechizos.bas `HechizoInvocacion` (Tipo 4) and `InvocarMascota` (Tipo 7, companions);
//! - AI_NPC.bas `SeguirAmo` / `AiNpcAtacaNpc`, MODULO_NPCs.bas `FollowAmo` / `QuitarMascota`,
//!   SistemaCombate.bas `CheckPets` / `AllFollowAmo`;
//! - General.bas `TiempoInvocacion` (summons expire) and the water elemental's healing;
//! - Modulo_UsUaRiOs.bas `UserDie` / `WarpMascotas` and TCP.bas `CloseUser` (what happens to
//!   pets when the owner dies, changes map or leaves);
//! - TCP_HandleData1.bas `ENTR` (trainer, NpcType 3) and TCP_HandleData2.bas `/QUITARMASCOTA`.
//!
//! The numbers (maximum pets, summon life, taming divisors) come from `init/profile.ini`
//! `[Pets]`; what each creature does as a pet from npcs.dat (`UniqueSummon`, `CastsOnNpcs`,
//! `HealsOwner`, `Companion`).
//!
//! Fixes over TSAO:
//! - `CheckPets(..., CheckElementales:=False)` (called when a creature attacks the owner)
//!   never set a target because of `If CheckElementales And ...`: pets did not defend their
//!   owner; here they do;
//! - the taming range compared the clicked tile with the creature's own tile (always 0):
//!   here the player must stand within 2 tiles;
//! - `SeguirAmo` stepped blindly towards the owner and got stuck on walls: here it uses the
//!   NPC path finder;
//! - a companion stayed behind on the old map when its owner travelled; here it goes away
//!   (cast it again), like summons;
//! - `/QUITARMASCOTA` only dismissed companions and there was no way to free a tamed pet:
//!   here it dismisses the companion, or else frees every pet;
//! - the earth elemental "cast" its spell 34 (Curar Veneno) on enemies, which
//!   `NpcLanzaSpellSobreNpc` ignores (only `SubeHP=2`): it never did anything; here a pet
//!   whose spell is not harmful fights hand to hand.
//!
//! Kept from TSAO: pets only fight creatures (the fire elemental's revenge on players,
//! `AllMascotasAtacanUser`, is not ported) and creatures do not fight back against pets,
//! except a dragon, which answers a casting elemental with its spell.
//!
//! Not ported: tamed pets are not saved with the character (the VB kept `MASCOTAS` in the
//! .chr file); they go back to the wild when the owner logs out.
use crate::clock::GameTime;
use crate::world::entities::EntityId;
use crate::world::World;
use ao_formats::dat::npcs::Movement;
use ao_formats::dat::spells::Spell;
use ao_protocol::{Tone, S2C};
use ao_rules::pathfinding::heading_to;

/// What a creature is to a player.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PetKind {
    #[default]
    Wild,
    /// Tamed with the skill (keeps its map slot: it respawns wild when it dies).
    Tamed,
    /// Summoned by a spell; vanishes after `SummonSecs`.
    Summoned,
    /// Follows and never fights (`Companion=1`).
    Companion,
    /// Brought by a trainer NPC (fights players like a wild one, never respawns).
    Trained,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PetState {
    pub kind: PetKind,
    pub owner: Option<EntityId>,
    /// `Contadores.TiempoExistencia`.
    pub expires: Option<GameTime>,
    /// Trainer that brought it (`MaestroNpc`).
    pub trainer: Option<u32>,
    /// `TargetNPC`.
    pub target_npc: Option<u32>,
    pub next_heal: GameTime,
    /// Created for a player or a trainer: removed when it dies, never respawned.
    pub no_respawn: bool,
}

/// `SeguirAmo`: farther than this from the owner, the pet walks back.
const FOLLOW_DISTANCE: i32 = 3;
/// `Case Domar`: the creature within this many tiles of the player.
const TAME_RANGE: i32 = 2;
/// A trainer answers `/entrenar` within this distance.
const TRAINER_RANGE: i32 = 4;

fn dist(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

impl World {
    fn pet_rules(&self) -> &'static ao_formats::profile::PetRules {
        &ao_formats::profile::for_recursos(&self.recursos).pets
    }

    /// Living pets of a player (they live on the owner's map), in a stable order.
    pub(crate) fn pets_of(&self, owner: u32) -> Vec<u32> {
        let Some(map) = self.player(owner).map(|p| p.map) else { return Vec::new() };
        let inst = self.instance(map);
        inst.npc_ids().iter().copied().filter(|nid| inst.npcs.get(nid).is_some_and(|n| n.alive() && n.pet.owner == Some(owner))).collect()
    }

    /// Pets that count against `[Pets] Max` (companions do not, like `NroMacotas`).
    fn fighting_pets(&self, owner: u32) -> Vec<u32> {
        let map = self.player(owner).map_or(0, |p| p.map);
        self.pets_of(owner).into_iter().filter(|nid| self.instance(map).npcs[nid].pet.kind != PetKind::Companion).collect()
    }

    /// Summoned creatures and one's own pets give no experience (see combat.rs).
    pub(crate) fn pet_gives_no_exp(&self, map: u16, nid: u32, attacker: u32) -> bool {
        self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).is_some_and(|n| matches!(n.pet.kind, PetKind::Summoned | PetKind::Companion) || n.pet.owner == Some(attacker))
    }

    /// `FollowAmo`: back to its owner's side, no target.
    pub(crate) fn pet_follow(&mut self, map: u16, nid: u32) {
        let Some(n) = self.maps.get_mut(&map).and_then(|m| m.npcs.get_mut(&nid)) else { return };
        if n.pet.owner.is_none() {
            return;
        }
        n.pet.target_npc = None;
        n.target = None;
        n.movement = Movement::FollowOwner;
        n.hostile = false;
    }

    /// Makes a creature `owner`'s pet of `kind`.
    fn adopt(&mut self, map: u16, nid: u32, owner: u32, kind: PetKind, expires: Option<GameTime>) {
        let n = self.npc_mut(map, nid);
        n.pet.owner = Some(owner);
        n.pet.kind = kind;
        n.pet.expires = expires;
        n.attacked_by = None;
        self.pet_follow(map, nid);
    }

    /// Takes a creature off the map with no loot and no respawn (`QuitarNPC`).
    fn remove_creature(&mut self, map: u16, nid: u32) {
        if let Some(inst) = self.maps.get_mut(&map) {
            if inst.remove_npc(nid).is_some() {
                self.broadcast(map, &S2C::Despawn { id: nid }, None);
            }
        }
    }

    /// The owner lets it go: created ones vanish, tamed ones go back to the wild
    /// (`MaestroUser = 0`, `OldMovement`, `OldHostil`).
    fn release_pet(&mut self, map: u16, nid: u32) {
        let Some(n) = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)) else { return };
        if n.pet.no_respawn || n.pet.kind != PetKind::Tamed {
            return self.remove_creature(map, nid);
        }
        let n = self.npc_mut(map, nid);
        n.pet = Default::default();
        n.movement = n.old_movement;
        n.hostile = n.old_hostile;
        n.target = None;
    }

    /// Creates creature `number` for `owner` near (x, y). None when it does not fit.
    fn spawn_for(&mut self, owner: Option<u32>, map: u16, number: u16, (x, y): (i32, i32), kind: PetKind, expires: Option<GameTime>) -> Option<u32> {
        let ao = self.ao.clone();
        let d = ao.npcs.get(number as u32).filter(|d| !d.name.is_empty())?;
        let (fx, fy) = self.free_tile_near(map, x, y, 2);
        if self.occupied(map, fx, fy) {
            return None;
        }
        let now = self.now();
        let mut npc = self.new_npc(number, d, fx, fy, now);
        npc.pet.no_respawn = true;
        npc.pet.kind = kind;
        let nid = self.next_npc;
        self.next_npc += 1;
        let msg = npc.spawn_msg(nid);
        self.maps.get_mut(&map)?.add_npc(nid, npc);
        self.broadcast(map, &msg, None);
        if let Some(o) = owner {
            self.adopt(map, nid, o, kind, expires);
        }
        Some(nid)
    }

    // ------------------------------------------------------------ taming

    /// `Case Domar` + `DoDomar`.
    pub(crate) fn tame(&mut self, id: u32, x: i32, y: i32) {
        let (map, px, py) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
        let Some(nid) = self.instance(map).npc_at(x, y) else { return self.console(id, "No hay ninguna criatura allí.", Tone::Info) };
        let n = &self.instance(map).npcs[&nid];
        let Some(d) = self.ao.npcs.get(n.def as u32) else { return };
        if d.tameable <= 0 {
            return self.console(id, "No puedes domar a esa criatura.", Tone::Warn);
        }
        if dist((px, py), (n.x, n.y)) > TAME_RANGE {
            return self.console(id, "Estás demasiado lejos.", Tone::Warn);
        }
        if n.pet.owner == Some(id) {
            return self.console(id, "La criatura ya te ha aceptado como su amo.", Tone::Info);
        }
        if n.pet.owner.is_some() || n.pet.trainer.is_some() {
            return self.console(id, "La criatura ya tiene amo.", Tone::Warn);
        }
        if n.attacked_by.is_some() {
            return self.console(id, "No puedes domar a una criatura que está peleando.", Tone::Warn);
        }
        if self.fighting_pets(id).len() >= self.pet_rules().max {
            return self.console(id, "No puedes controlar más criaturas.", Tone::Warn);
        }
        let need = d.tameable;
        if need <= self.taming_power(id) {
            self.adopt(map, nid, id, PetKind::Tamed, None);
            self.console(id, "La criatura te ha aceptado como su amo.", Tone::Good);
        } else {
            self.console(id, "No has logrado domar la criatura.", Tone::Info);
        }
    }

    /// `CalcularPoderDomador`: `CHA * (Domar / ModDomar) + 3 x RandomNumber(1, CHA / 3)`.
    fn taming_power(&mut self, id: u32) -> i32 {
        use ao_formats::dat::balance::{Class, Skill};
        let p = self.player(id).unwrap();
        let cha = p.stats.attributes[ao_rules::character::ATTR_CHARISMA] as f64;
        let skill = p.stats.skills.get(Skill::Taming.id() as usize - 1).copied().unwrap_or(0) as f64;
        let class = Class::from_id(p.class as i32).map_or("", |c| c.spanish_name());
        let m = self.pet_rules().tame_mod_for(class) as f64;
        let third = ao_rules::combat::vb_round(cha / 3.0).max(1) as i32;
        let mut power = ao_rules::combat::vb_round(cha * (skill / m)) as i32;
        for _ in 0..3 {
            power += self.roll(1, third);
        }
        power
    }

    // ------------------------------------------------------------ summoning

    /// `HechizoInvocacion` (Tipo 4) and `InvocarMascota` (Tipo 7) on tile (tx, ty).
    /// Returns true when the spell worked (the caller then charges it).
    pub(crate) fn cast_summon(&mut self, id: u32, s: &Spell, tx: i32, ty: i32, companion: bool) -> bool {
        let (map, px, py) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
        let number = s.npc_number;
        if number <= 0 || self.ao.npcs.get(number as u32).is_none() {
            self.console(id, "Ese hechizo no invoca nada.", Tone::Warn);
            return false;
        }
        if companion {
            // InvocarMascota: casting it again sends the companion away.
            let mine: Vec<u32> = self.pets_of(id).into_iter().filter(|n| self.instance(map).npcs[n].pet.kind == PetKind::Companion).collect();
            if !mine.is_empty() {
                for n in mine {
                    self.remove_creature(map, n);
                }
                self.console(id, "Tu mascota se ha ido.", Tone::Info);
                return false;
            }
            if self.occupied(map, tx, ty) || !ao_rules::walk::standable(&self.instance(map).map, tx, ty) {
                self.console(id, "No hay lugar para tu mascota allí.", Tone::Warn);
                return false;
            }
            return self.spawn_for(Some(id), map, number as u16, (tx, ty), PetKind::Companion, None).is_some();
        }
        // HechizoInvocacion: no fighting summons in safe zones (companions may go anywhere).
        let m = &self.instance(map).map;
        if m.safe || m.trigger(px, py) == ao_rules::combat::TRIGGER_SAFE_ZONE {
            self.console(id, "No puedes invocar criaturas en una zona segura.", Tone::Warn);
            return false;
        }
        let max = self.pet_rules().max;
        if self.fighting_pets(id).len() >= max {
            self.console(id, "No puedes controlar más criaturas.", Tone::Warn);
            return false;
        }
        if self.ao.npcs.get(number as u32).is_some_and(|d| d.special.unique_summon) && self.pets_of(id).iter().any(|n| self.instance(map).npcs[n].def as i32 == number) {
            self.console(id, "Ya tienes invocada una criatura de ese tipo.", Tone::Warn);
            return false;
        }
        let expires = Some(self.now().after_secs(self.pet_rules().summon_secs as u64));
        let mut any = false;
        for _ in 0..s.summon_count.max(1) {
            if self.fighting_pets(id).len() >= max {
                break;
            }
            any |= self.spawn_for(Some(id), map, number as u16, (tx, ty), PetKind::Summoned, expires).is_some();
        }
        any
    }

    // ------------------------------------------------------------ owner events

    /// `CheckPets`: the owner attacked `nid`; its fighting pets without a target go for it
    /// (healers stay out, as the water elemental did).
    pub(crate) fn pets_assist(&mut self, owner: u32, map: u16, nid: u32) {
        if self.player(owner).is_none_or(|p| p.map != map) {
            return;
        }
        // Never against a creature the owner may not attack (faction NPCs...).
        let number = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).map_or(0, |n| n.def);
        if self.faction_npc_attack_refusal(owner, number).is_some() {
            return;
        }
        for pet in self.pets_of(owner) {
            if pet == nid {
                continue;
            }
            let n = &self.instance(map).npcs[&pet];
            let healer = self.ao.npcs.get(n.def as u32).is_some_and(|d| d.special.heals_owner.is_some());
            if n.pet.kind == PetKind::Companion || healer || n.pet.target_npc.is_some() {
                continue;
            }
            let n = self.npc_mut(map, pet);
            n.pet.target_npc = Some(nid);
            n.movement = Movement::AttackNpc;
        }
    }

    /// A creature attacked the owner: the pets defend them (see "Fixes over TSAO"). Pets
    /// never fight creatures their owner may not attack.
    pub(crate) fn pets_defend(&mut self, owner: u32, map: u16, nid: u32) {
        let owned = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).is_some_and(|n| n.pet.owner.is_some());
        if !owned {
            self.pets_assist(owner, map, nid);
        }
    }

    /// `UserDie`: summons and companions vanish, tamed pets go back to the wild.
    pub(crate) fn pets_on_owner_death(&mut self, id: u32) {
        let Some(map) = self.player(id).map(|p| p.map) else { return };
        for nid in self.pets_of(id) {
            self.release_pet(map, nid);
        }
    }

    /// `CloseUser`: same as dying for the pets.
    pub(crate) fn pets_on_leave(&mut self, id: u32) {
        self.pets_on_owner_death(id);
    }

    /// `WarpMascotas`: the owner moved from `old` to its current map. Summons and companions
    /// vanish; tamed pets come along (a fresh one at the owner's side, with its health); the
    /// one left behind returns to the wild later as an ordinary respawn of its map.
    pub(crate) fn pets_on_map_change(&mut self, id: u32, old: u16) {
        let Some((map, x, y)) = self.player(id).map(|p| (p.map, p.x, p.y)) else { return };
        if map == old || !self.maps.contains_key(&old) {
            return;
        }
        let inst = self.instance(old);
        let left: Vec<u32> = inst.npc_ids().iter().copied().filter(|nid| inst.npcs.get(nid).is_some_and(|n| n.alive() && n.pet.owner == Some(id))).collect();
        let mut summoned = false;
        for nid in left {
            let n = &self.instance(old).npcs[&nid];
            let (def, kind, hp, no_respawn) = (n.def, n.pet.kind, n.hp, n.pet.no_respawn);
            if kind != PetKind::Tamed {
                summoned |= kind == PetKind::Summoned;
                self.remove_creature(old, nid);
                continue;
            }
            if no_respawn {
                self.remove_creature(old, nid);
            } else {
                // Leaves its tile; tick_respawns brings a wild one back after its interval.
                let now = self.now();
                let n = self.npc_mut(old, nid);
                n.pet = Default::default();
                n.dead_until = Some(now);
                self.broadcast(old, &S2C::Despawn { id: nid }, None);
            }
            if let Some(new) = self.spawn_for(Some(id), map, def, (x, y + 1), PetKind::Tamed, None) {
                self.npc_mut(map, new).hp = hp;
            }
        }
        if summoned {
            self.console(id, "Tus criaturas invocadas se han desvanecido.", Tone::Info);
        }
    }

    /// `/quitarmascota`: dismisses the companion, or else frees every pet.
    pub(crate) fn pets_dismiss(&mut self, id: u32) {
        let Some(map) = self.player(id).map(|p| p.map) else { return };
        let pets = self.pets_of(id);
        let companions: Vec<u32> = pets.iter().copied().filter(|n| self.instance(map).npcs[n].pet.kind == PetKind::Companion).collect();
        let list = if companions.is_empty() { pets } else { companions };
        if list.is_empty() {
            return self.console(id, "No tienes mascotas.", Tone::Info);
        }
        for nid in list {
            self.release_pet(map, nid);
        }
        self.console(id, "Has liberado a tus mascotas.", Tone::Info);
    }

    // ------------------------------------------------------------ trainer

    /// `ENTR`: `/entrenar` lists the creatures of the trainer next to the player,
    /// `/entrenar N` brings the N-th one (at most `[Pets] TrainerMax` out at once).
    pub(crate) fn trainer_command(&mut self, id: u32, arg: Option<&str>) {
        use ao_formats::dat::npcs::NpcType;
        let (map, px, py) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
        let inst = self.instance(map);
        let trainer = inst.npc_ids().iter().copied().find(|nid| inst.npcs.get(nid).is_some_and(|n| n.alive() && n.kind == NpcType::Trainer && dist((n.x, n.y), (px, py)) <= TRAINER_RANGE));
        let Some(tid) = trainer else { return self.console(id, "No hay ningún entrenador cerca.", Tone::Info) };
        let t = &self.instance(map).npcs[&tid];
        let (tx, ty, tname) = (t.x, t.y, t.name.clone());
        let Some(creatures) = self.ao.npcs.get(t.def as u32).map(|d| d.creatures.clone()) else { return };
        let Some(n) = arg.and_then(|a| a.parse::<usize>().ok()) else {
            let list: Vec<String> = creatures.iter().enumerate().map(|(i, c)| format!("{}) {}", i + 1, c.name)).collect();
            return self.console(id, format!("{tname}: puedo traerte {}. Escribe /entrenar y el número.", list.join(", ")), Tone::Info);
        };
        let Some(c) = n.checked_sub(1).and_then(|i| creatures.get(i)) else { return self.console(id, "No tengo esa criatura.", Tone::Warn) };
        let out = self.instance(map).npcs.values().filter(|n| n.alive() && n.pet.trainer == Some(tid)).count();
        if out >= self.pet_rules().trainer_max {
            let msg = S2C::NpcSay { id: tid, text: "No puedo traer más criaturas, ¡mata las existentes!".into() };
            return self.send(id, &msg);
        }
        if let Some(nid) = self.spawn_for(None, map, c.npc as u16, (tx, ty + 1), PetKind::Trained, None) {
            self.npc_mut(map, nid).pet.trainer = Some(tid);
        }
    }

    // ------------------------------------------------------------ development

    /// Development commands for bot tests (only reached with `dev_login`):
    /// `/skill <id> <valor>`, `/atributo <0-4> <valor>`, `/clase <n>`, `/aprender <hechizo>`,
    /// `/criatura <npc> [dx dy]` (a wild creature next to the player).
    pub(crate) fn dev_test_command(&mut self, id: u32, line: &str) -> bool {
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let n: Vec<i32> = parts.filter_map(|s| s.parse().ok()).collect();
        match (cmd, &n[..]) {
            ("/skill", [s, v]) if (1..=crate::world::stats::NUM_SKILLS as i32).contains(s) => {
                self.player_mut(id).stats.skills[*s as usize - 1] = (*v).clamp(0, 100) as u8;
            }
            ("/atributo", [a, v]) if (0..5).contains(a) => {
                self.player_mut(id).stats.attributes[*a as usize] = (*v).clamp(1, 60) as u8;
            }
            ("/llenar", []) => {
                let p = self.player_mut(id);
                p.hp = p.max_hp;
                p.mana = p.max_mana;
                p.stats.sta = p.stats.max_sta;
                self.send_stats(id);
                self.send_vitals(id);
            }
            ("/clase", [c]) if ao_formats::dat::balance::Class::from_id(*c).is_some() => {
                self.player_mut(id).class = *c as u8;
            }
            ("/aprender", [s]) if self.ao.spells.get(*s as u32).is_some() => {
                let _ = self.player_mut(id).spells.add(*s as u16);
                self.send_spellbook(id);
            }
            ("/criatura", [npc, rest @ ..]) if self.ao.npcs.get(*npc as u32).is_some() => {
                let (map, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
                let (dx, dy) = match rest {
                    [dx, dy] => (*dx, *dy),
                    _ => (0, 2),
                };
                let ao = self.ao.clone();
                let d = ao.npcs.get(*npc as u32).unwrap();
                let (fx, fy) = self.free_tile_near(map, x + dx, y + dy, 2);
                let now = self.now();
                let mut c = self.new_npc(*npc as u16, d, fx, fy, now);
                c.pet.no_respawn = true;
                let nid = self.next_npc;
                self.next_npc += 1;
                let msg = c.spawn_msg(nid);
                self.maps.get_mut(&map).unwrap().add_npc(nid, c);
                self.broadcast(map, &msg, None);
            }
            // Phase 7: an effect over time on oneself (AddOrResetEffect), for the buff bar.
            ("/efecto", [e]) if *e > 0 => self.add_or_reset_effect(id, *e),
            ("/skill" | "/atributo" | "/clase" | "/aprender" | "/criatura", _) => self.console(id, "Uso: /skill id valor, /atributo i valor, /clase n, /aprender hechizo, /criatura npc [dx dy].", Tone::Warn),
            _ => return false,
        }
        true
    }

    // ------------------------------------------------------------ AI

    /// Expired summons (`TiempoInvocacion`) and creature statuses, every tick.
    pub(crate) fn tick_pets(&mut self, map: u16, now: GameTime) {
        self.tick_npc_status(map, now);
        let inst = self.instance(map);
        let gone: Vec<u32> = inst.npc_ids().iter().copied().filter(|nid| inst.npcs.get(nid).is_some_and(|n| n.pet.expires.is_some_and(|t| now >= t))).collect();
        for nid in gone {
            self.remove_creature(map, nid);
        }
    }

    /// `SeguirAmo` / `AiNpcAtacaNpc` for a creature with an owner.
    pub(crate) fn pet_ai(&mut self, map: u16, nid: u32, now: GameTime) {
        let n = &self.instance(map).npcs[&nid];
        let Some(owner) = n.pet.owner else { return };
        let Some(op) = self.player(owner).filter(|p| p.map == map).map(|p| (p.x, p.y)) else { return };
        let ao = self.ao.clone();
        let Some(d) = ao.npcs.get(n.def as u32) else { return };
        let pos = (n.x, n.y);
        // The water elemental: heals its (druid) owner while hurt and near.
        if let Some(h) = &d.special.heals_owner {
            if now >= n.pet.next_heal && !self.is_dead(owner) {
                let p = self.player(owner).unwrap();
                let class_ok = h.class.as_ref().is_none_or(|c| ao_formats::dat::balance::Class::from_id(p.class as i32).is_some_and(|k| ao_formats::dat::obj::class_key(k.spanish_name()) == *c));
                if class_ok && p.hp < p.max_hp && dist(pos, op) < h.range {
                    self.npc_mut(map, nid).pet.next_heal = now.after_secs(h.secs as u64);
                    self.npc_cast_spell(map, nid, owner, h.spell as u16);
                }
            }
        }
        let n = &self.instance(map).npcs[&nid];
        if let Some(t) = n.pet.target_npc {
            let target = self.instance(map).npcs.get(&t).filter(|v| v.alive()).map(|v| (v.x, v.y));
            let Some(tp) = target.filter(|tp| (tp.0 - pos.0).abs() <= super::npc_ai::VISION_X && (tp.1 - pos.1).abs() <= super::npc_ai::VISION_Y) else {
                return self.pet_follow(map, nid);
            };
            // NpcLanzaSpellSobreNpc only does harm (`SubeHP=2`): the earth elemental's spell
            // (34, Curar Veneno) did nothing in TSAO, so here it fights hand to hand.
            let harmful = d.spells.first().and_then(|s| ao.spells.get(s.spell as u32)).is_some_and(|s| s.effects.contains(ao_formats::dat::spells::SpellEffects::DO_DAMAGE));
            if d.special.casts_on_npcs && harmful {
                if now >= n.next_spell {
                    self.npc_mut(map, nid).next_spell = now.after_ms(if d.spell_interval > 0 { d.spell_interval as u64 } else { 2000 });
                    self.npc_face(map, nid, heading_to(pos, tp));
                    self.npc_casts_on_npc(map, nid, t);
                    // AiNpcAtacaNpc: a dragon answers a casting elemental with its own spell.
                    if self.npc_paralyzed(map, t) {
                        return;
                    }
                    if self.instance(map).npcs.get(&t).is_some_and(|v| v.alive() && v.kind == ao_formats::dat::npcs::NpcType::Dragon) {
                        self.npc_casts_on_npc(map, t, nid);
                    }
                }
                return;
            }
            if dist(pos, tp) <= 1 {
                self.npc_face(map, nid, heading_to(pos, tp));
                if now >= self.instance(map).npcs[&nid].next_attack {
                    self.npc_mut(map, nid).next_attack = now.after_ms(if d.attack_interval > 0 { d.attack_interval as u64 } else { 2000 });
                    self.npc_attacks_npc(map, nid, t);
                }
            } else {
                self.walk_towards(map, nid, tp, now);
            }
            return;
        }
        if dist(pos, op) > FOLLOW_DISTANCE {
            self.walk_towards(map, nid, op, now);
        } else if self.rng.range(1, 12) == 3 {
            let dir = [ao_rules::Dir::Up, ao_rules::Dir::Right, ao_rules::Dir::Down, ao_rules::Dir::Left][self.rng.range(0, 3) as usize];
            self.npc_move(map, nid, dir);
        }
    }
}

/// World-level scenarios on the TSAO data (`AO_TSAO`, skipped without it): what bots cannot
/// see (owners, deaths, logout, the trainer's cap) or set up (classes, equipment).
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::clock::{Clock, VirtualClock};
    use crate::net::Event;
    use crate::Settings;
    use ao_protocol::{C2S, PROTOCOL_VERSION};

    pub(crate) struct T {
        pub w: World,
        pub clock: VirtualClock,
        pub t: GameTime,
    }

    pub(crate) fn tsao(map: u16, x: i32, y: i32) -> Option<T> {
        let rec = std::path::PathBuf::from(std::env::var("AO_TSAO").unwrap_or_else(|_| "X:/dev/tsao_rec".into()));
        if !rec.join("Mapas").join(format!("mapa{map}.csm")).exists() {
            eprintln!("TSAO data not found, skipping");
            return None;
        }
        let mut s = Settings::defaults();
        s.recursos = rec;
        let clock = VirtualClock::new();
        clock.set(GameTime(10_000));
        s.clock = Clock::Virtual(clock.clone());
        s.start = Some((map, x, y));
        let mut w = World::new(s);
        assert!(w.ensure_map(map));
        Some(T { w, clock, t: GameTime(10_000) })
    }

    impl T {
        pub fn join(&mut self, id: u32, name: &str) {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            std::mem::forget(rx);
            self.w.on_event(Event::Connected { id, addr: "127.0.0.1:9".parse().unwrap(), tx });
            self.w.handle(id, C2S::Login { name: name.into(), version: PROTOCOL_VERSION });
            assert!(self.w.player(id).is_some());
            self.w.player_mut(id).faction.safe = false;
        }

        pub fn run(&mut self, ms: u64) {
            let end = self.t.after_ms(ms);
            while self.t < end {
                self.t = self.t.after_ms(50);
                self.clock.set(self.t);
                self.w.tick(self.t);
            }
        }

        /// A wild creature (no respawn) near (x, y).
        pub fn creature(&mut self, map: u16, number: u16, x: i32, y: i32) -> u32 {
            let ao = self.w.ao.clone();
            let d = ao.npcs.get(number as u32).unwrap();
            let (fx, fy) = self.w.free_tile_near(map, x, y, 2);
            let mut n = self.w.new_npc(number, d, fx, fy, self.t);
            n.pet.no_respawn = true;
            let id = self.w.next_npc;
            self.w.next_npc += 1;
            self.w.maps.get_mut(&map).unwrap().add_npc(id, n);
            id
        }

        pub fn npc(&self, map: u16, nid: u32) -> Option<&crate::world::entities::Npc> {
            self.w.instance(map).npcs.get(&nid).filter(|n| n.alive())
        }
    }

    const MAP: u16 = 1;

    fn summon(t: &mut T, id: u32, spell: u16, x: i32, y: i32) -> Option<u32> {
        let s = t.w.ao.spells.get(spell as u32).unwrap().clone();
        assert!(t.w.cast_summon(id, &s, x, y, false));
        t.w.pets_of(id).into_iter().find(|n| t.w.instance(MAP).npcs[n].def as i32 == s.npc_number)
    }

    #[test]
    fn fire_elemental_burns_what_its_owner_hits_and_owner_gets_the_kill() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Druida");
        let fire = summon(&mut t, 1, 26, 52, 50).expect("fire elemental");
        let pig = t.creature(MAP, 3, 50, 52);
        t.w.pets_assist(1, MAP, pig);
        assert_eq!(t.npc(MAP, fire).unwrap().pet.target_npc, Some(pig));
        t.run(3000);
        assert!(t.npc(MAP, pig).is_none(), "the pig burnt");
        assert_eq!(t.npc(MAP, fire).unwrap().pet.target_npc, None, "back to following");
    }

    #[test]
    fn pets_defend_their_owner_from_creatures() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Amo");
        t.w.player_mut(1).stats.skills[23] = 100;
        let ant = t.creature(MAP, 76, 51, 50);
        let (x, y) = { let n = t.npc(MAP, ant).unwrap(); (n.x, n.y) };
        t.w.tame(1, x, y);
        assert_eq!(t.npc(MAP, ant).unwrap().pet.owner, Some(1));
        let wolf = t.creature(MAP, 501, 50, 52);
        // The wolf bites the owner: the ant turns on it (TSAO's CheckPets never did).
        t.w.npc_attacks_player(MAP, wolf, 1);
        assert_eq!(t.npc(MAP, ant).unwrap().pet.target_npc, Some(wolf));
    }

    #[test]
    fn summons_die_with_the_owner_tamed_pets_go_wild_on_logout() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Fugaz");
        t.w.player_mut(1).stats.skills[23] = 100;
        let earth = summon(&mut t, 1, 28, 52, 50).expect("earth elemental");
        let ant = t.creature(MAP, 76, 51, 51);
        let (x, y) = { let n = t.npc(MAP, ant).unwrap(); (n.x, n.y) };
        // A map creature that respawns: tamed, then released on logout it stays as a wild one.
        t.w.npc_mut(MAP, ant).pet.no_respawn = false;
        t.w.tame(1, x, y);
        assert_eq!(t.w.pets_of(1).len(), 2);
        t.w.user_die(1);
        assert!(t.npc(MAP, earth).is_none(), "the summon died with its owner");
        let n = t.npc(MAP, ant).unwrap();
        assert_eq!((n.pet.owner, n.pet.kind), (None, PetKind::Wild), "the tamed ant is wild again");

        t.w.resurrect(1);
        let (x, y) = { let n = t.npc(MAP, ant).unwrap(); (n.x, n.y) };
        t.w.tame(1, x, y);
        assert_eq!(t.npc(MAP, ant).unwrap().pet.owner, Some(1));
        t.w.on_event(Event::Disconnected { id: 1 });
        assert_eq!(t.npc(MAP, ant).unwrap().pet.owner, None, "logout frees it");
    }

    #[test]
    fn max_pets_and_one_elemental_of_each_kind() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Legion");
        let s17 = t.w.ao.spells.get(17).unwrap().clone(); // Invokar Zombies: 2 at once
        assert!(t.w.cast_summon(1, &s17, 52, 50, false));
        assert_eq!(t.w.pets_of(1).len(), 2);
        assert!(t.w.cast_summon(1, &s17, 52, 52, false));
        assert_eq!(t.w.pets_of(1).len(), 3, "[Pets] Max=3");
        assert!(!t.w.cast_summon(1, &s17, 48, 50, false));
        // Expiry after SummonSecs.
        t.run(121_000);
        assert!(t.w.pets_of(1).is_empty());
    }

    #[test]
    fn own_summons_give_no_experience() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Granjero");
        let s17 = t.w.ao.spells.get(17).unwrap().clone();
        assert!(t.w.cast_summon(1, &s17, 52, 50, false));
        let zombie = t.w.pets_of(1)[0];
        let exp0 = t.w.player(1).unwrap().exp;
        t.w.damage_npc(MAP, zombie, 100, 1);
        assert_eq!(t.w.player(1).unwrap().exp, exp0, "TSAO zombies gave 1000 exp to their own summoner");
    }

    #[test]
    fn companions_come_even_in_town_fighting_summons_do_not() {
        // Tanaris (map 28) is a safe city.
        let Some(mut t) = tsao(28, 50, 50) else { return };
        t.join(1, "Paseante");
        let (x, y) = { let p = t.w.player(1).unwrap(); (p.x, p.y) };
        let (cx, cy) = t.w.free_tile_near(28, x + 1, y, 2);
        let companion = t.w.ao.spells.get(51).unwrap().clone();
        assert!(t.w.cast_summon(1, &companion, cx, cy, true));
        let zombies = t.w.ao.spells.get(17).unwrap().clone();
        assert!(!t.w.cast_summon(1, &zombies, cx, cy + 1, false));
        assert_eq!(t.w.pets_of(1).len(), 1);
    }

    #[test]
    fn trainer_brings_up_to_its_cap() {
        let Some(mut t) = tsao(MAP, 50, 50) else { return };
        t.join(1, "Alumno");
        let trainer = t.creature(MAP, 60, 50, 52);
        let cap = t.w.pet_rules().trainer_max;
        for _ in 0..cap + 2 {
            t.w.trainer_command(1, Some("1"));
        }
        let out = t.w.instance(MAP).npcs.values().filter(|n| n.pet.trainer == Some(trainer)).count();
        assert_eq!(out, cap);
        assert!(t.w.instance(MAP).npcs.values().filter(|n| n.pet.trainer == Some(trainer)).all(|n| n.pet.owner.is_none() && n.pet.no_respawn));
    }
}
