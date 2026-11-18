//! Transporter NPCs: paid trips between cities (TSAO `/VIAJAR` + `frmViajar`, AO20
//! `Transporter`). The destinations are data (`CityN` in npcs.dat), not code.
//!
//! Fixes over TSAO: the VB handler validated the name against a hard-coded list, charged
//! the gold AFTER warping and returned early for two cities (free trips to Anvilmar and
//! Kahlimdor), and trusted the last clicked NPC. Here the trip is checked as a whole
//! (alive, near, level, gold) before anything changes, and the gold is taken first.
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_protocol::p4::travel::{TravelC2S, TravelDest, TravelS2C};
use ao_protocol::{Tone, S2C};

/// TSAO `Distancia(...) > 5`.
const TRAVEL_RANGE: i32 = 5;
/// From this level on the trip costs `price_high` (TSAO: 1.000 -> 5.000 gold).
const HIGH_PRICE_LEVEL: u32 = 30;

impl World {
    /// The trips NPC instance `nid` (on the player's map) offers to player `id`.
    fn travel_offer(&self, id: u32, nid: u32) -> Option<(String, Vec<TravelDest>)> {
        let p = self.player(id)?;
        let n = self.instance(p.map).npcs.get(&nid)?;
        if n.kind != NpcType::Transporter {
            return None;
        }
        let d = self.ao.npcs.get(n.def as u32)?;
        let dests = d
            .cities
            .iter()
            .map(|c| {
                let high = p.level >= HIGH_PRICE_LEVEL && c.price_high > 0;
                TravelDest { name: c.name.clone(), map: c.map.max(0) as u16, price: (if high { c.price_high } else { c.price }).max(0) as u32, min_level: c.min_level.max(d.transporter_level).max(0) as u32 }
            })
            .collect();
        Some((d.name.clone(), dests))
    }

    /// HOOK (interact): the player clicked a transporter that is alive and near.
    pub(crate) fn travel_npc(&mut self, id: u32, nid: u32) {
        if let Some((name, dests)) = self.travel_offer(id, nid) {
            self.send(id, &S2C::Travel(TravelS2C::Open { npc: nid, name, dests }));
        }
    }

    pub(crate) fn on_travel(&mut self, id: u32, m: TravelC2S) {
        let TravelC2S::Go { npc, index } = m;
        let close = |w: &World| w.send(id, &S2C::Travel(TravelS2C::Close));
        let Some(p) = self.player(id) else { return };
        let (map, px, py, level, gold, dead) = (p.map, p.x, p.y, p.level, p.inv.gold, p.hp <= 0);
        let Some(n) = self.instance(map).npcs.get(&npc) else { return close(self) };
        if (n.x - px).abs() + (n.y - py).abs() > TRAVEL_RANGE {
            self.console(id, "Estás demasiado lejos del viajero.", Tone::Info);
            return close(self);
        }
        if dead {
            self.console(id, "Estás muerto!!", Tone::Warn);
            return close(self);
        }
        let Some((_, dests)) = self.travel_offer(id, npc) else { return close(self) };
        let Some(dest) = dests.get(index as usize).cloned() else {
            return self.strike(id, "viaje inexistente");
        };
        let Some(city) = self.ao.npcs.get(self.instance(map).npcs[&npc].def as u32).and_then(|d| d.cities.get(index as usize)).cloned() else { return };
        if level < dest.min_level {
            return self.console(id, format!("Necesitas nivel {} para viajar a {}.", dest.min_level, dest.name), Tone::Warn);
        }
        if gold < dest.price {
            return self.console(id, format!("Necesitas {} monedas de oro para viajar a {}.", dest.price, dest.name), Tone::Warn);
        }
        if !self.ensure_map(dest.map) {
            return self.console(id, "Ese destino no está disponible.", Tone::Warn);
        }
        self.player_mut(id).inv.gold -= dest.price;
        self.send_inventory_vitals(id);
        close(self);
        self.console(id, format!("Viajas a {} por {} monedas de oro.", dest.name, dest.price), Tone::Good);
        self.teleport(id, dest.map, city.x, city.y);
    }
}
