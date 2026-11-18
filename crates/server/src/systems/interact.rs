//! Clicking on the world: talking to NPCs (banker, priest, merchant...). OWNER: NPC system
//! (P1, Opus D). Ported from `GameLogic.LookatTile` (the NPC description) and
//! `WorldActions.HandleWorldAction` / `HandleNpcInteractionByType` (the action), which the
//! AO20 client sends on click and double click; here one `Interact` does both.
use crate::systems::npc_ai::{VISION_X, VISION_Y};
use crate::world::World;
use ao_formats::csm::MAP_SIZE;
use ao_formats::dat::npcs::NpcType;
use ao_protocol::{Tone, S2C};

const TOO_FAR: &str = "Estas demasiado lejos.";
const DEAD: &str = "Estas muerto!!";

fn distancia(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

impl World {
    pub(crate) fn on_interact(&mut self, id: u32, tx: i32, ty: i32) {
        let Some(p) = self.player(id) else { return };
        let (map, px, py, dead, level) = (p.map, p.x, p.y, p.hp <= 0, p.level);
        // IsWorldActionRequestValid. A tile outside the map cannot come from a real client
        // (strike); one outside the visible area is ignored (a stale click after walking).
        if !(1..=MAP_SIZE).contains(&tx) || !(1..=MAP_SIZE).contains(&ty) {
            return self.strike(id, "interaccion fuera del mapa");
        }
        if (px - tx).abs() > VISION_X || (py - ty).abs() > VISION_Y {
            return;
        }
        if !self.instance(map).populated {
            self.populate_map(map);
        }
        let Some(nid) = self.instance(map).npc_at(tx, ty) else {
            // Map objects with an action: the anvil opens the blacksmith (craft system).
            if self.door_click(id, tx, ty) || self.craft_click_tile(id, tx, ty) || self.floor_click(id, map, tx, ty) || self.tsao_click_player(id, map, tx, ty) {
                return;
            }
            self.console(id, "No hay nada con que interactuar ahi.", Tone::Info);
            return;
        };
        let ao = self.ao.clone();
        let n = &self.instance(map).npcs[&nid];
        let (npos, kind) = ((n.x, n.y), n.kind);
        let Some(d) = ao.npcs.get(n.def as u32) else { return };
        let dist = distancia((px, py), npos);
        // LookatTile updates "visit this NPC" quest counters for any NPC clicked.
        self.quest_on_npc_target(id, nid);

        // LookatTile: the description over its head.
        if d.description.chars().count() > 1 {
            if !dead || kind == NpcType::Reviver {
                let near_only = matches!(kind, NpcType::Quest | NpcType::Banker | NpcType::Reviver | NpcType::Common | NpcType::Trainer | NpcType::Governor);
                let euclid = (((px - npos.0).pow(2) + (py - npos.1).pow(2)) as f64).sqrt();
                if !near_only || euclid < 3.0 {
                    self.send(id, &S2C::NpcSay { id: nid, text: d.description.clone() });
                }
            }
        } else if !dead {
            self.console(id, d.name.clone(), Tone::Info);
        }

        // HandleNpcInteractionByType (merchants are checked before the type).
        if d.trades == 1 {
            if self.npc_alive_and_near(id, dead, dist, 4) {
                self.open_npc_trade(id, nid);
            }
        } else if kind == NpcType::Banker {
            if self.npc_alive_and_near(id, dead, dist, 4) {
                self.open_bank(id, nid);
            }
        } else if matches!(kind, NpcType::Reviver | NpcType::NewbieReviver) {
            self.priest(id, kind, dist, dead, level);
        } else if kind == NpcType::Quest {
            if self.npc_alive_and_near(id, dead, dist, 4) {
                self.quest_npc(id, nid);
            }
        } else if matches!(kind, NpcType::Recruiter | NpcType::Governor) {
            if self.npc_alive_and_near(id, dead, dist, 4) {
                self.faction_npc(id, nid, kind);
            }
        } else if d.crafts > 0 {
            if self.npc_alive_and_near(id, dead, dist, 4) {
                self.craft_npc(id, nid);
            }
        } else if kind == NpcType::Transporter {
            if self.npc_alive_and_near(id, dead, dist, 5) {
                self.travel_npc(id, nid);
            }
        } else if kind == NpcType::PostOffice {
            if self.npc_alive_and_near(id, dead, dist, 5) {
                self.mail_npc(id, nid);
            }
        } else if kind == NpcType::ClanVault {
            if self.npc_alive_and_near(id, dead, dist, 5) {
                self.clan_vault_open(id, nid);
            }
        } else if kind == NpcType::Surgeon {
            if self.npc_alive_and_near(id, dead, dist, 5) {
                self.surgeon_open(id, nid);
            }
        } else if kind == NpcType::Auctioneer && !dead {
            // Auctions run from anywhere (event system, window U); no official map places one.
            self.console(id, "Las subastas están abiertas: pulsa U para verlas.", Tone::Info);
        } else if !dead
            && (matches!(
                    kind,
                    NpcType::Pirate | NpcType::Governor | NpcType::FishDelivery | NpcType::Ao20Shop | NpcType::Ao20ShopPjs | NpcType::EventMaster
                ))
        {
            // Travel (ships), homes, fish delivery, AO20 shops: not ported yet.
            self.console(id, "Esa interacción todavía no está disponible.", Tone::Info);
        }
    }

    /// `EnsureUserAliveForNpcInteraction` + `EnsureNpcWithinDistance`.
    fn npc_alive_and_near(&self, id: u32, dead: bool, dist: i32, max: i32) -> bool {
        if dead {
            self.console(id, DEAD, Tone::Warn);
            false
        } else if dist > max {
            self.console(id, TOO_FAR, Tone::Info);
            false
        } else {
            true
        }
    }

    /// `HandleReviverNpcInteraction`: within 5 tiles, resurrects the dead (any player at a
    /// priest, newbies at a newbie priest) or heals the living to full.
    fn priest(&mut self, id: u32, kind: NpcType, dist: i32, dead: bool, level: u32) {
        if dist > 5 {
            self.console(id, TOO_FAR, Tone::Info);
            return;
        }
        // Flags.Envenenado / Incinerado are cleared here in AO20: status effects belong to
        // the magic system (P1 B), not wired yet. MAP_HOME_IN_JAIL is P2.
        if dead && (kind == NpcType::Reviver || level <= self.data.balance.newbie_max_level) {
            self.console(id, "Has sido resucitado!", Tone::Good);
            self.resurrect(id);
        } else if !dead {
            let Some(p) = self.player(id) else { return };
            if p.hp != p.max_hp {
                let max = p.max_hp;
                self.heal_player(id, max);
                self.console(id, "El sacerdote lanza unas palabras al aire. Comienzas a sentir como tu cuerpo vuelve a estar sano.", Tone::Good);
            }
        }
    }
}
