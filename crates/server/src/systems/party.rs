//! Party system. OWNER: Opus C (docs/P2_CONTRATO.md).
//!
//! Port of ModGrupos.bas (`InvitarMiembro`, `EcharMiembro`, `SalirDeGrupo`,
//! `SalirDeGrupoForzado`, `FinalizarGrupo`, `GroupCreateSuccess`, `AddUserToGRoup`),
//! Protocol.bas (`WorkLeftClick` e_Skill.Grupo, `HandleResponderPregunta` question 1,
//! `HandleAbandonarGrupo`, `HandlePartyToggle`), TCP.bas (`ResetUserSlot`) and
//! SistemaCombate.bas (`CalcularDarExpGrupal`).
use crate::world::World;
use ao_protocol::social::party::PartyMember;
use ao_protocol::{PartyC2S, PartyS2C, Tone, S2C};
use ao_rules::party::{self as rules, ShareMember};

impl World {
    pub(crate) fn on_party(&mut self, id: u32, msg: PartyC2S) {
        if self.player(id).is_none() {
            return;
        }
        match msg {
            PartyC2S::Invite { target } => self.party_invite(id, target),
            PartyC2S::Respond { accept } => self.party_respond(id, accept),
            PartyC2S::Leave => self.party_abandon(id),
            PartyC2S::Kick { member } => self.party_kick(id, member),
            PartyC2S::ToggleSafe => {
                let p = self.player_mut(id);
                p.party.safe = !p.party.safe;
                let on = p.party.safe;
                self.send(id, &S2C::Party(PartyS2C::Safe { on }));
                let text = if on { "Seguro de grupo activado." } else { "Seguro de grupo desactivado." };
                self.console(id, text, Tone::Info);
            }
        }
    }

    /// HOOK (combat): experience earned by `id` from an NPC on `map`. The party system
    /// shares it among nearby members (`CalcularDarExpGrupal`); alone it all goes to `id`.
    pub(crate) fn share_exp(&mut self, id: u32, amount: u32, map: u16) {
        let Some(party) = self.party_of(id).and_then(|pid| self.parties.by_id.get(&pid)).cloned() else {
            self.give_exp(id, amount);
            return;
        };
        let Some(me) = self.player(id) else { return };
        let earner = (map, me.x, me.y);
        let leader_map = self.player(party.leader).map_or(0, |p| p.map);
        let members: Vec<ShareMember> = party
            .members
            .iter()
            .map(|&m| match self.player(m) {
                Some(p) => ShareMember {
                    alive: p.hp > 0 && !p.combat.dead,
                    pos: (p.map, p.x, p.y),
                    leadership: rules::has_leadership(p.stats.skills[rules::SKILL_LEADERSHIP], p.stats.attributes[rules::ATTR_CHARISMA]),
                },
                None => ShareMember { alive: false, pos: (0, 0, 0), leadership: false },
            })
            .collect();
        if leader_map != map {
            for (&m, s) in party.members.iter().zip(&members) {
                if s.alive {
                    self.console(m, "El líder del grupo está demasiado lejos, su experiencia se pierde.", Tone::Combat);
                }
            }
        }
        let shares = rules::share_exp(amount, earner, leader_map, &members);
        for (&m, exp) in party.members.iter().zip(shares) {
            if exp > 0 {
                self.console(m, format!("El grupo ha ganado {exp} puntos de experiencia."), Tone::Combat);
                self.give_exp(m, exp);
            }
        }
    }

    /// HOOK (chat channels): ids of the online members of `id`'s party, `id` included.
    /// Empty if not in a party.
    pub(crate) fn party_members_online(&self, id: u32) -> Vec<u32> {
        self.party_of(id)
            .and_then(|pid| self.parties.by_id.get(&pid))
            .map(|p| p.members.iter().copied().filter(|m| self.player(*m).is_some()).collect())
            .unwrap_or_default()
    }

    /// Called when the player enters the world (after everything else is sent).
    pub(crate) fn party_on_enter(&mut self, id: u32) {
        // `ConnectUser`: SeguroParty = True.
        let Some(p) = self.player(id) else { return };
        let on = p.party.safe;
        self.send(id, &S2C::Party(PartyS2C::Safe { on }));
    }

    /// Called right before the player leaves the world (logout or kick), still present.
    /// `ResetUserSlot`: the leader disbands (`FinalizarGrupo`), a member leaves
    /// (`SalirDeGrupoForzado`); invitations from this player are dropped.
    pub(crate) fn party_on_leave(&mut self, id: u32) {
        self.party_drop_from_group(id);
        let invited: Vec<u32> = self.sessions.iter().filter_map(|(k, s)| s.player.as_ref().filter(|p| p.party.invited_by == Some(id)).map(|_| *k)).collect();
        for k in invited {
            self.player_mut(k).party.invited_by = None;
        }
    }

    // ------------------------------------------------------------ helpers for others

    /// `Grupo.Id` of the player, if in a party.
    pub(crate) fn party_of(&self, id: u32) -> Option<u64> {
        self.player(id).and_then(|p| p.party.id)
    }

    /// `VolverCriminal` / `VolverCiudadano`: the leader disbands, a member leaves.
    pub(crate) fn party_drop_from_group(&mut self, id: u32) {
        let Some(pid) = self.party_of(id) else { return };
        let Some(party) = self.parties.by_id.get(&pid) else {
            self.player_mut(id).party.id = None;
            return;
        };
        if party.leader == id {
            self.party_disband(pid);
        } else {
            self.party_remove(pid, id, Removal::Left);
        }
    }

    // ------------------------------------------------------------ internals

    fn party_name(&self, id: u32) -> String {
        self.player(id).map_or_else(String::new, |p| p.name.clone())
    }

    fn party_send_info(&self, pid: u64) {
        let Some(party) = self.parties.by_id.get(&pid) else { return };
        let members: Vec<PartyMember> = party.members.iter().map(|&m| PartyMember { id: m, name: self.party_name(m) }).collect();
        let msg = S2C::Party(PartyS2C::Members { leader: party.leader, members });
        for &m in &party.members {
            self.send(m, &msg);
        }
    }

    fn party_send_none(&self, id: u32) {
        self.send(id, &S2C::Party(PartyS2C::Members { leader: 0, members: Vec::new() }));
    }

    /// `WorkLeftClick` (e_Skill.Grupo) + `InvitarMiembro`.
    fn party_invite(&mut self, id: u32, target: u32) {
        if self.is_dead(id) {
            self.console(id, "¡¡Estás muerto!!.", Tone::Warn);
            return;
        }
        if target == id || self.player(target).is_none() {
            self.console(id, "No hay ningún usuario ahí.", Tone::Info);
            return;
        }
        let my_party = self.party_of(id).and_then(|pid| self.parties.by_id.get(&pid)).cloned();
        if let Some(party) = &my_party {
            if party.leader != id {
                let leader = self.party_name(party.leader);
                self.console(id, format!("Tu no podés invitar usuarios, debe hacerlo {leader}."), Tone::Warn);
                return;
            }
        }
        if self.is_dead(target) {
            self.console(id, "El usuario está muerto.", Tone::Info);
            return;
        }
        let (me, other) = (self.player(id).unwrap(), self.player(target).unwrap());
        if me.map != other.map || (me.x - other.x).abs() + (me.y - other.y).abs() > rules::INVITE_DISTANCE {
            self.console(id, "Estás demasiado lejos.", Tone::Info);
            return;
        }
        if other.party.safe {
            self.console(id, "El usuario debe desactivar el seguro de grupos para poder invitarlo.", Tone::Info);
            return;
        }
        let count = my_party.as_ref().map_or(1, |p| p.members.len());
        if count >= rules::MAX_MEMBERS {
            self.console(id, format!("No puedes invitar a más personas, el límite es {}.", rules::MAX_MEMBERS), Tone::Info);
            return;
        }
        if !rules::factions_compatible(me.faction.rec.status, other.faction.rec.status) {
            self.console(id, "No podes crear un grupo con personajes de diferentes facciones.", Tone::Info);
            return;
        }
        let lead = rules::has_leadership(me.stats.skills[rules::SKILL_LEADERSHIP], me.stats.attributes[rules::ATTR_CHARISMA]);
        if !rules::level_gap_ok(me.level, other.level, lead) {
            let max = if lead { rules::PARTY_ELV_LEADERSHIP } else { rules::PARTY_ELV };
            self.console(id, format!("No podes crear un grupo con personajes con diferencia de más de {max} niveles."), Tone::Info);
            return;
        }
        if other.party.id.is_some() {
            self.console(id, "El personaje ya se encuentra en un grupo.", Tone::Info);
            return;
        }
        if other.party.invited_by.is_some() {
            self.console(id, "El usuario tiene una solicitud pendiente.", Tone::Info);
            return;
        }
        let name = me.name.clone();
        self.player_mut(target).party.invited_by = Some(id);
        self.console(id, "Se envió la solicitud de grupo, espera la respuesta.", Tone::Info);
        self.console(target, format!("{name} te invitó a unirse a su grupo."), Tone::Info);
        self.send(target, &S2C::Party(PartyS2C::Invited { from: id, name }));
    }

    /// `HandleResponderPregunta` (question 1) + `GroupCreateSuccess` + `AddUserToGRoup`.
    fn party_respond(&mut self, id: u32, accept: bool) {
        let Some(from) = self.player_mut(id).party.invited_by.take() else {
            self.console(id, "Servidor » Solicitud de grupo invalida, reintente...", Tone::Info);
            return;
        };
        if !accept {
            self.console(from, "El usuario no esta interesado en formar parte del grupo.", Tone::Info);
            self.console(id, "Has rechazado la propuesta.", Tone::Info);
            return;
        }
        if self.party_of(id).is_some() {
            self.console(id, "¡Ya estás en un grupo!", Tone::Info);
            return;
        }
        if self.player(from).is_none() {
            self.console(id, "¡El grupo ya no existe!", Tone::Info);
            return;
        }
        let pid = match self.party_of(from) {
            Some(pid) => {
                let p = &self.parties.by_id[&pid];
                if p.leader != from {
                    self.console(id, "¡El lider del grupo ha cambiado, imposible unirse!", Tone::Info);
                    return;
                }
                // The original checks the size only when inviting; two pending invitations
                // must not overfill the party.
                if p.members.len() >= rules::MAX_MEMBERS {
                    self.console(id, "¡El grupo está completo!", Tone::Info);
                    return;
                }
                pid
            }
            None => {
                let pid = self.parties.create(from);
                self.player_mut(from).party.id = Some(pid);
                self.console(from, "¡El grupo ha sido creado!", Tone::Good);
                pid
            }
        };
        let name = self.party_name(id);
        for m in self.parties.by_id[&pid].members.clone() {
            self.console(m, format!("{name} ha ingresado al grupo."), Tone::Info);
        }
        self.parties.by_id.get_mut(&pid).unwrap().members.push(id);
        self.player_mut(id).party.id = Some(pid);
        self.console(id, "¡Has sido añadido al grupo!", Tone::Good);
        self.party_send_info(pid);
    }

    /// `HandleAbandonarGrupo`.
    fn party_abandon(&mut self, id: u32) {
        let Some(pid) = self.party_of(id) else {
            self.console(id, "No estás en ningún grupo.", Tone::Info);
            return;
        };
        if self.parties.by_id.get(&pid).is_some_and(|p| p.leader == id) {
            self.party_disband(pid);
        } else {
            self.party_remove(pid, id, Removal::Left);
        }
    }

    /// `EcharMiembro` (by entity id instead of the list index).
    fn party_kick(&mut self, id: u32, member: u32) {
        let Some(pid) = self.party_of(id) else {
            self.console(id, "No estás en ningún grupo.", Tone::Info);
            return;
        };
        let party = &self.parties.by_id[&pid];
        if party.leader != id {
            self.console(id, "Solo el líder puede echar usuarios del grupo.", Tone::Warn);
            return;
        }
        if member == id {
            self.console(id, "No podés expulsarte a ti mismo.", Tone::Info);
            return;
        }
        if !party.members.contains(&member) {
            self.console(id, "Ese usuario no está en tu grupo.", Tone::Info);
            return;
        }
        self.party_remove(pid, member, Removal::Kicked);
    }

    /// `SalirDeGrupo` / `SalirDeGrupoForzado` / the member part of `EcharMiembro`; a party
    /// left with only its leader is disbanded.
    fn party_remove(&mut self, pid: u64, member: u32, how: Removal) {
        let Some(party) = self.parties.by_id.get_mut(&pid) else { return };
        party.members.retain(|&m| m != member);
        let (leader, left) = (party.leader, party.members.len());
        let name = self.party_name(member);
        if self.player(member).is_some() {
            self.player_mut(member).party.id = None;
            self.party_send_none(member);
        }
        match how {
            Removal::Left => {
                self.console(member, "Has sido removido del grupo.", Tone::Info);
                self.console(leader, format!("{name} ha abandonado el grupo."), Tone::Info);
            }
            Removal::Kicked => {
                self.console(leader, format!("{name} fue expulsado del grupo."), Tone::Info);
                self.console(member, "Has sido removido del grupo.", Tone::Info);
            }
        }
        if left <= 1 {
            self.parties.by_id.remove(&pid);
            if self.player(leader).is_some() {
                self.player_mut(leader).party.id = None;
                self.console(leader, "El grupo se ha disuelto, todos los miembros se fueron.", Tone::Info);
                self.party_send_none(leader);
            }
        } else {
            self.party_send_info(pid);
        }
    }

    /// `FinalizarGrupo`.
    fn party_disband(&mut self, pid: u64) {
        let Some(party) = self.parties.by_id.remove(&pid) else { return };
        for m in party.members {
            if self.player(m).is_none() {
                continue;
            }
            self.player_mut(m).party.id = None;
            self.party_send_none(m);
            let text = if m == party.leader { "Has disuelto el grupo." } else { "El líder ha abandonado el grupo. El grupo se disuelve." };
            self.console(m, text, Tone::Info);
        }
    }
}

#[derive(Clone, Copy)]
enum Removal {
    Left,
    Kicked,
}
