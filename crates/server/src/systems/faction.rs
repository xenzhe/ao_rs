//! Faction system. OWNER: Opus C (docs/P2_CONTRATO.md).
//!
//! Port of ModFacciones.bas (`EnlistarArmadaReal`, `RecompensaArmadaReal`, `EnlistarCaos`,
//! `RecompensaCaos`, `ExpulsarFaccionReal`, `ExpulsarFaccionCaos`, `DarRecompensas`,
//! `PerderItemsFaccionarios`), WorldActions.bas (`HandleFactionRecruiterNpcInteraction`),
//! Protocol.bas (`HandleLeaveFaction`, `HandleSafeToggle`, `HandleLegionarySecure`),
//! SistemaCombate.bas (`PuedeAtacar`, `UsuarioAtacadoPorUsuario`) and Modulo_UsUaRiOs.bas
//! (`ContarMuerte`, `VolverCriminal`, `AlreadyKilledBy`, `RegisterRecentKiller`).
use crate::world::faction::{tables, MAX_RECENT_KILLERS};
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_protocol::{FactionC2S, FactionS2C, Tone, S2C};
use ao_rules::faction::{self as rules, Fighter};

/// npcs.dat `Faccion` of a recruiter: 0 = royal army, anything else = chaos legion.
const NPC_FACTION_ROYAL: i32 = 0;
/// `e_Class.Thief`.
const CLASS_THIEF: u8 = 11;

impl World {
    pub(crate) fn on_faction(&mut self, id: u32, msg: FactionC2S) {
        if self.player(id).is_none() {
            return;
        }
        match msg {
            FactionC2S::Leave { npc } => self.faction_leave(id, npc),
            FactionC2S::ToggleSafe => {
                // HandleSafeToggle (guild alignment part: guilds are another system).
                if !rules::is_citizen(self.faction_status(id)) {
                    self.console(id, "Solo los ciudadanos pueden cambiar el seguro.", Tone::Info);
                    return;
                }
                let p = self.player_mut(id);
                p.faction.safe = !p.faction.safe;
                let on = p.faction.safe;
                self.send(id, &S2C::Faction(FactionS2C::Safe { on }));
                let text = if on { "Has activado el seguro." } else { "Has desactivado el seguro." };
                self.console(id, text, Tone::Info);
            }
            FactionC2S::ToggleLegionSafe => {
                let p = self.player_mut(id);
                p.faction.legion_safe = !p.faction.legion_safe;
                let on = p.faction.legion_safe;
                self.send(id, &S2C::Faction(FactionS2C::LegionSafe { on }));
            }
        }
    }

    /// HOOK (interact): the player talked to a faction NPC (recruiter or governor).
    pub(crate) fn faction_npc(&mut self, id: u32, npc: u32, kind: NpcType) {
        if kind != NpcType::Recruiter {
            // HandleGovernorNpcInteraction sets the home city (`Hogar`), not a faction.
            self.console(id, "Los hogares llegan más adelante.", Tone::Info);
            return;
        }
        let Some(royal) = self.faction_recruiter(id, npc, true) else { return };
        let status = self.faction_status(id);
        // HandleFactionRecruiterNpcInteraction
        match (royal, rules::is_royal(status), rules::is_chaos(status)) {
            (true, false, _) => self.faction_enlist_royal(id, npc),
            (true, true, _) => self.faction_reward(id, npc, true),
            (false, _, false) => self.faction_enlist_chaos(id, npc),
            (false, _, true) => self.faction_reward(id, npc, false),
        }
    }

    /// HOOK (combat, magic): why `attacker` may not hurt the player `victim`, if so
    /// (`PuedeAtacar` faction/criminal rules beyond safe zones). The caller shows the
    /// message to the attacker. Default: allowed.
    pub(crate) fn faction_attack_refusal(&self, attacker: u32, victim: u32) -> Option<&'static str> {
        let (a, v) = (self.faction_fighter(attacker)?, self.faction_fighter(victim)?);
        let map = self.player(victim)?.map;
        let safe = self.maps.get(&map).is_some_and(|m| m.map.safe);
        rules::attack_refusal(a, v, safe)
    }

    /// HOOK (combat): `killer` killed the player `victim` (criminal status, faction
    /// kills...). Called after the death is processed.
    pub(crate) fn faction_on_player_killed(&mut self, killer: u32, victim: u32) {
        if killer == victim || self.player(killer).is_none() || self.player(victim).is_none() {
            return;
        }
        // The hit that killed is an attack: UsuarioAtacadoPorUsuario first.
        self.faction_on_player_attacked(killer, victim);
        // ContarMuerte
        let now = self.now();
        let rekill_ms = self.ao.balance.extra.faction_rekill_time.max(0) as u64;
        let (k, v) = (self.player(killer).unwrap(), self.player(victim).unwrap());
        let killer_name = k.name.clone();
        let victim_name = v.name.clone();
        let rekill = v.faction.recent_killers.iter().any(|(n, t)| *n == killer_name && now.0.saturating_sub(t.0) < rekill_ms);
        let Some(c) = rules::count_kill(k.faction.rec.status, k.level, v.faction.rec.status, v.level, ao_rules::items::is_newbie(v.level), rekill) else { return };
        if !rekill {
            let rk = &mut self.player_mut(victim).faction.recent_killers;
            rk.retain(|(n, _)| *n != killer_name);
            rk.push((killer_name, now));
            if rk.len() > MAX_RECENT_KILLERS {
                rk.remove(0);
            }
        }
        let f = &mut self.player_mut(killer).faction.rec;
        f.criminals_killed = (f.criminals_killed + c.criminals).min(rules::MAX_USER_KILLS);
        f.citizens_killed = (f.citizens_killed + c.citizens).min(rules::MAX_USER_KILLS);
        f.score = (f.score + c.score).max(0);
        if c.score > 0 {
            self.console(killer, format!("Has ganado {} puntos de facción por matar a {victim_name}.", c.score), Tone::Good);
        } else if c.score < 0 {
            self.console(killer, format!("Has perdido {} puntos de facción.", -c.score), Tone::Warn);
        }
        self.faction_send_state(killer);
        self.faction_save(killer);
    }

    /// HOOK (combat): `killer` killed an NPC of npcs.dat `npc_number` (faction NPC kills).
    /// `MuereNpc` keeps no faction counters; what remains is `UserCanAttackNpc.TurnPK`
    /// (a citizen attacking an army NPC turns criminal), applied here on the kill until
    /// combat calls [`World::faction_on_npc_attacked`].
    pub(crate) fn faction_on_npc_killed(&mut self, killer: u32, npc_number: u16) {
        self.faction_on_npc_attacked(killer, npc_number);
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn faction_on_enter(&mut self, id: u32) {
        let Some(char_id) = self.player(id).and_then(|p| p.char_id) else {
            self.faction_after_enter(id);
            return;
        };
        match self.db.load_faction(char_id) {
            Ok(rec) => self.player_mut(id).faction.rec = rec,
            Err(e) => eprintln!("faccion de {char_id}: {e:?}"),
        }
        self.faction_after_enter(id);
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    pub(crate) fn faction_on_leave(&mut self, id: u32) {
        self.faction_save(id);
    }

    // ------------------------------------------------------------ helpers for others

    /// `Status(UserIndex)` (`e_Facciones`): for the NPC system (`EsEnemigo`), parties...
    pub(crate) fn faction_status(&self, id: u32) -> u8 {
        self.player(id).map_or(rules::CITIZEN, |p| p.faction.rec.status)
    }

    /// `UsuarioAtacadoPorUsuario` (faction part): a citizen hitting a citizen or an army
    /// member turns criminal. NOT CONNECTED yet: combat/magic should call it on every hit
    /// against a player; today it runs on the killing blow (`faction_on_player_killed`).
    pub(crate) fn faction_on_player_attacked(&mut self, attacker: u32, victim: u32) {
        if rules::attack_turns_criminal(self.faction_status(attacker), self.faction_status(victim)) {
            self.faction_turn_criminal(attacker);
        }
    }

    /// `UserCanAttackNpc` (TurnPK part): a citizen attacking an army NPC turns criminal.
    /// NOT CONNECTED yet (combat should call it on every hit against an NPC).
    pub(crate) fn faction_on_npc_attacked(&mut self, attacker: u32, npc_number: u16) {
        let npc_faction = self.ao.npcs.get(npc_number as u32).map_or(-1, |d| d.faction);
        if rules::is_citizen(self.faction_status(attacker)) && (npc_faction == rules::ROYAL as i32 || npc_faction == rules::COUNCIL_ROYAL as i32) {
            self.faction_turn_criminal(attacker);
        }
    }

    /// `UserCanAttackNpc` (faction part): army/legion members may not attack NPCs of their
    /// own faction. NOT CONNECTED yet (melee/magic against NPCs).
    #[allow(dead_code)]
    pub(crate) fn faction_npc_attack_refusal(&self, attacker: u32, npc_number: u16) -> Option<&'static str> {
        let status = self.faction_status(attacker);
        let npc_faction = self.ao.npcs.get(npc_number as u32).map_or(-1, |d| d.faction);
        ((rules::is_royal(status) || rules::is_chaos(status)) && npc_faction == status as i32).then_some("No podés atacar a miembros de tu facción.")
    }

    /// `VolverCriminal`.
    pub(crate) fn faction_turn_criminal(&mut self, id: u32) {
        let status = self.faction_status(id);
        if rules::is_chaos(status) || rules::is_criminal(status) {
            return;
        }
        let f = &mut self.player_mut(id).faction.rec;
        if rules::is_citizen(status) {
            f.score = 0;
        }
        f.status = rules::CRIMINAL;
        self.console(id, "Ahora sos un criminal.", Tone::Warn);
        self.faction_refresh(id);
        if self.party_of(id).is_some() {
            self.console(id, "Ahora sos criminal, no podés estar en un grupo con ciudadanos.", Tone::Warn);
            self.party_drop_from_group(id);
        }
        self.faction_save(id);
    }

    // ------------------------------------------------------------ internals

    fn faction_fighter(&self, id: u32) -> Option<Fighter> {
        let p = self.player(id)?;
        Some(Fighter { status: p.faction.rec.status, safe: p.faction.safe, legion_safe: p.faction.legion_safe, party: p.party.id.unwrap_or(0) })
    }

    fn faction_after_enter(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let (safe, legion_safe, map) = (p.faction.safe, p.faction.legion_safe, p.map);
        self.send(id, &S2C::Faction(FactionS2C::Safe { on: safe }));
        self.send(id, &S2C::Faction(FactionS2C::LegionSafe { on: legion_safe }));
        self.faction_send_state(id);
        // Everyone's faction status on the map, both ways (RefreshCharStatus).
        let others: Vec<(u32, u8)> = self.sessions.iter().filter_map(|(k, s)| s.player.as_ref().filter(|o| o.map == map && *k != id).map(|o| (*k, o.faction.rec.status))).collect();
        for (other, status) in others {
            self.send(id, &S2C::Faction(FactionS2C::Status { id: other, status }));
        }
        self.faction_refresh(id);
    }

    /// `RefreshCharStatus`: the map learns the new status.
    fn faction_refresh(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        self.broadcast(p.map, &S2C::Faction(FactionS2C::Status { id, status: p.faction.rec.status }), None);
        self.faction_send_state(id);
    }

    fn faction_send_state(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let f = &p.faction.rec;
        let rank = f.rank();
        let t = tables(&self.recursos);
        let title = if rank > 0 { t.rank(rules::is_royal(f.status), rank).map_or_else(String::new, |r| r.title.clone()) } else { rules::status_name(f.status).to_string() };
        self.send(
            id,
            &S2C::Faction(FactionS2C::State { status: f.status, rank, title, score: f.score, citizens_killed: f.citizens_killed, criminals_killed: f.criminals_killed }),
        );
    }

    fn faction_save(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let Some(char_id) = p.char_id else { return };
        if let Err(e) = self.db.save_faction(char_id, &p.faction.rec) {
            eprintln!("guardar faccion de {char_id}: {e:?}");
        }
    }

    /// The NPC speaks to the player (`WriteLocaleChatOverHead`); also in the console.
    fn faction_say(&self, id: u32, npc: u32, text: impl Into<String>) {
        let text = text.into();
        self.send(id, &S2C::NpcSay { id: npc, text: text.clone() });
        self.console(id, text, Tone::Chat);
    }

    /// The recruiter `npc` of the player's map, alive and within 3 tiles: `Some(royal)`.
    /// `talk`: say why not (else silent).
    fn faction_recruiter(&self, id: u32, npc: u32, talk: bool) -> Option<bool> {
        let p = self.player(id)?;
        let n = self.maps.get(&p.map)?.npcs.get(&npc).filter(|n| n.alive())?;
        let d = self.ao.npcs.get(n.def as u32)?;
        if d.npc_type != NpcType::Recruiter {
            return None;
        }
        if (n.x - p.x).abs() + (n.y - p.y).abs() > rules::RECRUITER_DISTANCE {
            if talk {
                self.console(id, "Estás demasiado lejos.", Tone::Info);
            }
            return None;
        }
        Some(d.faction == NPC_FACTION_ROYAL)
    }

    fn faction_has_free_slot(&self, id: u32) -> bool {
        self.player(id).is_some_and(|p| p.inv.slots.iter().take(p.inv.usable_slots() as usize).any(|s| s.is_none()))
    }

    /// `EnlistarArmadaReal` (the guild-alignment part belongs to guilds).
    fn faction_enlist_royal(&mut self, id: u32, npc: u32) {
        let t = tables(&self.recursos);
        let Some(first) = t.rank(true, 1).cloned() else {
            self.faction_say(id, npc, "El Ejército Real no recluta por ahora.");
            return;
        };
        let p = self.player(id).unwrap();
        let f = &p.faction.rec;
        let refusal = if rules::is_royal(f.status) {
            Some("Ya perteneces a mi ejército jóven soldado. Ve a combatir el caos en mis tierras para subir de rango en el Ejército Real.".to_string())
        } else if rules::is_chaos(f.status) {
            Some("¡Has llegado al lugar equivocado maldita escoria! Vete de aquí antes de ser encarcelado e interrogado.".into())
        } else if rules::is_criminal(f.status) {
            Some("No se permiten criminales en el Ejército Real.".into())
        } else if p.class == CLASS_THIEF {
            Some("No hay lugar para escoria en el Ejército Real.".into())
        } else if f.citizens_killed > 0 {
            Some("Has derramado sangre inocente. Solo aceptamos almas nobles en el Ejército Real.".into())
        } else if !self.faction_has_free_slot(id) {
            Some("¡No tienes lugar suficiente en el inventario.".into())
        } else if f.enlistments > rules::MAX_FACTION_ENLISTMENTS {
            Some("Ya has desertado el Ejército Real. No serás aceptado otra vez.".into())
        } else if f.score < first.score {
            Some(format!("Para unirte a las fuerzas de la Armada Real debes tener al menos {} puntos de facción, de momento sólo tienes {}.", first.score, f.score))
        } else if p.level < first.level {
            Some(format!("¡¡¡Para unirte a nuestras fuerzas debes ser al menos de nivel {}!!!", first.level))
        } else {
            None
        };
        if let Some(why) = refusal {
            self.faction_say(id, npc, why);
            return;
        }
        let level = p.level;
        let f = &mut self.player_mut(id).faction.rec;
        f.royal_rank = first.rank;
        f.enlistments += 1;
        f.status = rules::ROYAL;
        let first_time = f.royal_rewarded == 0;
        if first_time {
            f.entry_level = level;
            f.entry_kills = f.citizens_killed;
        }
        if first_time {
            self.faction_say(id, npc, "¡¡¡Bienvenido al Ejercito Imperial!!!, aqui tienes tus vestimentas. Cumple bien tu labor exterminando Criminales y me encargaré de recompensarte.");
        }
        self.console(id, "¡Ahora perteneces al Ejército Real!", Tone::Good);
        self.faction_give_rewards(id);
        self.faction_refresh(id);
        self.faction_save(id);
    }

    /// `EnlistarCaos`.
    fn faction_enlist_chaos(&mut self, id: u32, npc: u32) {
        let t = tables(&self.recursos);
        let Some(first) = t.rank(false, 1).cloned() else {
            self.faction_say(id, npc, "La Legión Oscura no recluta por ahora.");
            return;
        };
        let p = self.player(id).unwrap();
        let f = &p.faction.rec;
        let refusal = if rules::is_chaos(f.status) {
            Some("Ya perteneces a la Legión Oscura.".to_string())
        } else if rules::is_lawful(f.status) {
            Some("¡¡Ja ja ja!! Tu no eres bienvenido aquí asqueroso Ciudadano".into())
        } else if !self.faction_has_free_slot(id) {
            Some("¡No tienes lugar suficiente en el inventario.".into())
        } else if f.enlistments > rules::MAX_FACTION_ENLISTMENTS {
            Some("Has sido expulsado de las fuerzas oscuras y durante tu rebeldía has atacado a mi ejército. ¡Vete de aquí!".into())
        } else if f.score < first.score {
            Some(format!("Para unirte a las fuerzas de la Legión Oscura debes tener al menos {} puntos de facción, de momento sólo tienes {}.", first.score, f.score))
        } else if p.level < first.level {
            Some(format!("¡¡¡Para unirte a nuestras fuerzas debes ser al menos de nivel {}!!!", first.level))
        } else {
            None
        };
        if let Some(why) = refusal {
            self.faction_say(id, npc, why);
            return;
        }
        let level = p.level;
        let f = &mut self.player_mut(id).faction.rec;
        f.chaos_rank = first.rank;
        f.enlistments += 1;
        f.status = rules::CHAOS;
        let first_time = f.chaos_rewarded == 0;
        if first_time {
            f.entry_level = level;
            self.faction_say(id, npc, "Aquí tienes tu armadura legionario, ve a derramar sangre de los súbditos de Tancredo. Esta guerra será larga y cruel.");
        }
        self.console(id, "¡Ahora perteneces a la Legión Oscura!", Tone::Good);
        self.faction_give_rewards(id);
        self.faction_refresh(id);
        self.faction_save(id);
    }

    /// `RecompensaArmadaReal` / `RecompensaCaos`.
    fn faction_reward(&mut self, id: u32, npc: u32, royal: bool) {
        let t = tables(&self.recursos);
        let p = self.player(id).unwrap();
        let f = &p.faction.rec;
        let rank = if royal { f.royal_rank } else { f.chaos_rank };
        if rank >= t.max_rank() {
            let text = if royal {
                "Has alcanzado el máximo rango dentro de mis soldados. Demuestra tu bondad y liderazgo en el campo de batalla para algún día pertenecer al Consejo de Banderbill."
            } else {
                "¡Has alcanzado uno de los mejores lugares en mis filas. Mantén firme tu liderazgo y crueldad para algún día formar parte del Concilio de las Sombras."
            };
            self.faction_say(id, npc, text);
            return;
        }
        if !self.faction_has_free_slot(id) {
            self.faction_say(id, npc, "¡No tienes lugar suficiente en el inventario.");
            return;
        }
        let Some(next) = t.next_rank(royal, rank).cloned() else { return };
        if f.score < next.score {
            self.faction_say(id, npc, format!("Te faltan {} Puntos de faccion para subir de rango.", next.score - f.score));
            return;
        }
        if next.level > p.level {
            let missing = next.level - p.level;
            let text = if royal {
                format!("Has matado suficientes criminales pero, te faltan {missing} niveles para poder recibir la próxima recompensa.")
            } else {
                format!("Has acabado con la vida de suficientes enemigos pero aún te faltan {missing} niveles para alcanzar el siguiente rango.")
            };
            self.faction_say(id, npc, text);
            return;
        }
        let f = &mut self.player_mut(id).faction.rec;
        if royal {
            f.royal_rank = next.rank;
        } else {
            f.chaos_rank = next.rank;
        }
        let text = if royal {
            format!("¡¡¡Aquí tienes tu recompensa {}!!!", next.title)
        } else {
            format!("¡¡¡Bien hecho {}, aquí tienes tu recompensa, sigue pregonando el caos a lo largo de estas tierras.!!!", next.title)
        };
        self.faction_say(id, npc, text);
        self.faction_give_rewards(id);
        self.faction_send_state(id);
        self.faction_save(id);
    }

    /// `DarRecompensas`: the rewards of every rank above the last one rewarded, the ones
    /// this character can use; to the floor when the inventory is full.
    fn faction_give_rewards(&mut self, id: u32) {
        let t = tables(&self.recursos);
        let p = self.player(id).unwrap();
        let f = p.faction.rec.clone();
        let (rank, last) = if rules::is_royal(f.status) {
            (f.royal_rank, f.royal_rewarded)
        } else if rules::is_chaos(f.status) {
            (f.chaos_rank, f.chaos_rewarded)
        } else {
            return;
        };
        if last >= rank {
            return;
        }
        let (class, gender, race, level, map, x, y) = (p.class, p.gender, p.race, p.level, p.map, p.x, p.y);
        let objs: Vec<u16> = t
            .rewards
            .iter()
            .filter(|r| r.rank <= rank && r.rank > last)
            .filter(|r| self.ao.objs.get(r.obj as u32).is_some_and(|o| rules::reward_usable(o, f.status, rank, level, class, gender, race)))
            .map(|r| r.obj)
            .collect();
        for obj in objs {
            let item = InvItem { obj, amount: 1 };
            if !self.add_item(id, item) {
                self.drop_to_floor(map, x, y, item, false);
            }
        }
        let f = &mut self.player_mut(id).faction.rec;
        if rules::is_royal(f.status) {
            f.royal_rewarded = rank;
        } else {
            f.chaos_rewarded = rank;
        }
    }

    /// `PerderItemsFaccionarios`.
    fn faction_lose_items(&mut self, id: u32) {
        let Some(p) = self.player(id) else { return };
        let slots: Vec<(u8, u32)> = p
            .inv
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.filter(|it| self.ao.objs.get(it.obj as u32).is_some_and(|o| o.royal == 1 || o.chaos == 1)).map(|it| ((i + 1) as u8, it.amount)))
            .collect();
        for (slot, amount) in slots {
            self.remove_from_slot(id, slot, amount);
        }
    }

    /// `ExpulsarFaccionReal` (royal = true) / `ExpulsarFaccionCaos`.
    fn faction_expel(&mut self, id: u32, royal: bool) {
        self.player_mut(id).faction.rec.status = if royal { rules::CITIZEN } else { rules::CRIMINAL };
        self.faction_refresh(id);
        self.faction_lose_items(id);
        let text = if royal { "Has sido expulsado del Ejército Real." } else { "Has sido expulsado de la Legión Oscura." };
        self.console(id, text, Tone::Warn);
        self.faction_save(id);
    }

    /// `HandleLeaveFaction` (the guild parts belong to guilds).
    fn faction_leave(&mut self, id: u32, npc: Option<u32>) {
        if self.is_dead(id) {
            self.console(id, "¡¡Estás muerto!!.", Tone::Warn);
            return;
        }
        let status = self.faction_status(id);
        if rules::is_citizen(status) {
            self.faction_turn_criminal(id);
            return;
        }
        let recruiter = npc.and_then(|n| self.faction_recruiter(id, n, false).map(|royal| (n, royal)));
        let Some((npc, royal_npc)) = recruiter else {
            if rules::is_royal(status) {
                self.console(id, "Para salir del ejercito debes ir a visitar al rey.", Tone::Info);
            } else if rules::is_chaos(status) {
                self.console(id, "Para salir de la legion debes ir a visitar al diablo.", Tone::Info);
            }
            return;
        };
        if rules::is_royal(status) {
            if royal_npc {
                self.faction_expel(id, true);
                self.faction_say(id, npc, "Serás bienvenido a las fuerzas imperiales si deseas regresar.");
            } else {
                self.faction_say(id, npc, "¡¡¡Sal de aquí bufón!!!");
            }
        } else if rules::is_chaos(status) {
            if !royal_npc {
                self.faction_expel(id, false);
                self.faction_say(id, npc, "Ya volverás arrastrandote.");
            } else {
                self.faction_say(id, npc, "Sal de aquí maldito criminal");
            }
        } else {
            self.faction_say(id, npc, "¡No perteneces a ninguna facción!");
        }
    }
}
