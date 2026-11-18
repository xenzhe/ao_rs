//! Who a creature belongs to while it is being fought, and creature poison, ported from
//! `MODULO_NPCs.bas` (`GetOwnedBy`, `CanAttackNotOwner`, the ownership part of
//! `UserCanAttackNpc`, `NpcEnvenenarUser`) and `Modulo_UsUaRiOs.NPCAtacado`.
//!
//! A creature hit by a citizen is "theirs" for `IntervaloNpcOwner` (10 s) after each of
//! their hits. Another player outside their party and guild may only steal it with the
//! "seguro" off, and doing so turns them criminal; while it is owned, the creature only
//! goes for players who could attack it without becoming criminal.
use crate::world::World;
use ao_protocol::Tone;
use ao_rules::faction as frules;

/// `IntervaloNpcOwner` (Configuracion.ini default).
pub const NPC_OWNER_MS: u64 = 10_000;
/// `NpcEnvenenarUser`: `RandomNumber(1, 100) < 30`.
const NPC_POISON_CHANCE_BELOW: i32 = 30;

/// What `UserCanAttackNpc` says about the owner of a creature.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OwnerRule {
    /// Free to attack.
    Free,
    /// Owned by a citizen: attacking turns the attacker criminal (`TurnPK`).
    TurnsCriminal,
    /// Same, with the "seguro" on: refused (`eRemoveSafeCitizenNpc`).
    SafeOn,
}

impl World {
    /// `GetOwnedBy`: the player who owns the creature now, if any.
    pub(crate) fn npc_owner(&self, map: u16, nid: u32) -> Option<u32> {
        let now = self.now();
        let (owner, until) = self.maps.get(&map)?.npcs.get(&nid)?.status.owner?;
        (now < until && self.player(owner).is_some_and(|p| !p.combat.dead && p.hp > 0)).then_some(owner)
    }

    /// The ownership part of `UserCanAttackNpc` for player `id`.
    pub(crate) fn npc_owner_rule(&self, id: u32, map: u16, nid: u32) -> OwnerRule {
        // An AO20 rule: the TSAO server has no creature owners.
        if self.tsao_rules() || self.player(id).is_none() {
            return OwnerRule::Free;
        }
        let Some(owner) = self.npc_owner(map, nid).filter(|o| *o != id) else { return OwnerRule::Free };
        let Some(n) = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)) else { return OwnerRule::Free };
        if self.ao.npcs.get(n.def as u32).is_some_and(|d| d.attackable_by_everyone != 0) || n.pet.owner.is_some() {
            return OwnerRule::Free;
        }
        let s = self.faction_status(owner);
        if !(frules::is_citizen(s) || frules::is_royal(s) || s == frules::COUNCIL_ROYAL) {
            return OwnerRule::Free;
        }
        let (a, o) = (self.player(id).unwrap(), self.player(owner).unwrap());
        let same_guild = a.guild.guild.is_some() && a.guild.guild == o.guild.guild;
        let same_party = self.party_of(id).is_some() && self.party_of(id) == self.party_of(owner);
        if same_guild || same_party {
            return OwnerRule::Free;
        }
        if a.faction.safe {
            OwnerRule::SafeOn
        } else {
            OwnerRule::TurnsCriminal
        }
    }

    /// Before a swing, shot or spell on a creature (`UserCanAttackNpc`): true = refused
    /// (seguro on against another citizen's creature), with the warning. Nothing is spent.
    pub(crate) fn npc_owner_refuses(&self, id: u32, map: u16, nid: u32) -> bool {
        if self.npc_owner_rule(id, map, nid) == OwnerRule::SafeOn {
            self.console(id, "Esa criatura está peleando con otro ciudadano. Quitá el seguro para atacarla.", Tone::Warn);
            return true;
        }
        false
    }

    /// Called on every hit of a player on a creature (`damage_npc`, `NPCAtacado`): a thief
    /// turns criminal, then the attacker becomes the owner. False = the hit does not happen
    /// (only reached with the seguro on by paths that skipped `npc_owner_refuses`).
    pub(crate) fn npc_owner_on_hit(&mut self, id: u32, map: u16, nid: u32) -> bool {
        match self.npc_owner_rule(id, map, nid) {
            OwnerRule::SafeOn => return false,
            OwnerRule::TurnsCriminal => self.faction_turn_criminal(id),
            OwnerRule::Free => {}
        }
        let until = self.now().after_ms(NPC_OWNER_MS);
        if let Some(n) = self.maps.get_mut(&map).and_then(|m| m.npcs.get_mut(&nid)) {
            n.status.owner = Some((id, until));
        }
        true
    }

    /// `CanAttackNotOwner`: may creature `nid`, fighting its owner, turn on player `pid`?
    /// Only if `pid` could hit it without turning criminal.
    pub(crate) fn npc_may_attack_not_owner(&self, map: u16, nid: u32, pid: u32) -> bool {
        match self.npc_owner(map, nid) {
            Some(o) if o != pid => self.npc_owner_rule(pid, map, nid) == OwnerRule::Free,
            _ => true,
        }
    }

    /// `NpcEnvenenarUser`: a creature with `Veneno` poisons on a hit, 29 % of the time.
    pub(crate) fn npc_poison_on_hit(&mut self, map: u16, nid: u32, pid: u32) {
        let poison = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)).and_then(|n| self.ao.npcs.get(n.def as u32)).map_or(0, |d| d.poison);
        if !self.cfg.world.creature_poison || poison <= 0 || self.is_dead(pid) || self.rng.range(1, 100) >= NPC_POISON_CHANCE_BELOW {
            return;
        }
        let now = self.now();
        let st = &mut self.player_mut(pid).status;
        if !st.poisoned {
            st.poisoned = true;
            st.poison_next = now.after_ms(ao_rules::magic::POISON_PERIOD_MS);
        }
        self.console(pid, "¡¡La criatura te ha envenenado!!", Tone::Combat);
        self.refresh_status(pid);
    }
}

#[cfg(test)]
mod tests {
    use super::super::eot::tests::{T, MAP};
    use super::OwnerRule;

    #[test]
    fn a_citizens_creature_cannot_be_stolen_with_the_seguro_on() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.join(2, 3, 52, 50);
        t.dummy(900_040, 51, 50);
        assert!(t.w.npc_owner_on_hit(1, MAP, 900_040));
        assert_eq!(t.w.npc_owner(MAP, 900_040), Some(1));
        t.w.player_mut(2).faction.safe = true;
        assert_eq!(t.w.npc_owner_rule(2, MAP, 900_040), OwnerRule::SafeOn);
        assert!(!t.w.npc_owner_on_hit(2, MAP, 900_040));
        assert!(!t.w.npc_may_attack_not_owner(MAP, 900_040, 2), "it keeps fighting its owner");
        t.w.player_mut(2).faction.safe = false;
        assert!(t.w.npc_owner_on_hit(2, MAP, 900_040), "a steal without the seguro...");
        assert_eq!(t.w.faction_status(2), ao_rules::faction::CRIMINAL, "...turns criminal");
        assert_eq!(t.w.npc_owner(MAP, 900_040), Some(2));
        t.advance(super::NPC_OWNER_MS);
        assert_eq!(t.w.npc_owner(MAP, 900_040), None, "10 s without hitting it: free again");
    }

    /// Through the real entry points: the refused swing shows nothing and the refused
    /// spell costs nothing.
    #[test]
    fn a_refused_steal_spends_nothing() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        t.join(2, 1, 52, 50);
        t.dummy(900_042, 51, 50);
        assert!(t.w.npc_owner_on_hit(1, MAP, 900_042));
        t.w.player_mut(2).faction.safe = true;
        t.w.player_mut(2).dir = ao_rules::Dir::Left;
        t.advance(2_000);
        t.drain(2);
        t.seq += 1;
        t.w.on_melee(2, t.seq);
        let msgs = t.drain(2);
        assert!(!msgs.iter().any(|m| matches!(m, ao_protocol::S2C::Attack { attacker: 2, .. })), "no swing shown");
        assert_eq!(t.w.instance(MAP).npcs[&900_042].hp, 100_000);
        t.learn(2, 1);
        let mana = t.mana(2);
        t.cast(2, 1, 51, 50);
        assert_eq!(t.mana(2), mana, "Dardo Mágico refused for free");
        assert_eq!(t.w.instance(MAP).npcs[&900_042].hp, 100_000);
    }

    #[test]
    fn poisonous_creatures_poison_some_hits() {
        let mut t = T::new();
        t.join(1, 3, 50, 50);
        let number = t.w.ao.npcs.iter().find(|(_, d)| d.poison > 0).map(|(i, _)| i as u16).expect("a poisonous creature");
        let npc = t.w.make_npc(number, 51, 50).unwrap();
        t.w.maps.get_mut(&MAP).unwrap().add_npc(900_041, npc);
        let mut poisoned = false;
        for _ in 0..60 {
            t.w.npc_poison_on_hit(MAP, 900_041, 1);
            if t.w.player(1).unwrap().status.poisoned {
                poisoned = true;
                break;
            }
        }
        assert!(poisoned);
    }
}
