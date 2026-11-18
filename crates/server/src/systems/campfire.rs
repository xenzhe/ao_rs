//! Campfires: a dagger turns a pile of wood into an unlit campfire
//! (`Trabajo.bas::TratarDeHacerFogata`), a click lights it (`HandleCampfireTwigAction`) and
//! rain puts lit ones out on rainy maps (`General.bas::ApagarFogatas`).
use crate::world::inventory::InvItem;
use crate::world::stats::SKILL_SURVIVAL;
use crate::world::World;
use ao_formats::dat::balance::Skill;
use ao_protocol::Tone;

/// `Wood`, `DAGA`, `FOGATA_APAG`, `FOGATA` (Declares.bas).
pub const WOOD: u16 = 58;
pub const DAGGER: u16 = 15;
pub const CAMPFIRE_UNLIT: u16 = 136;
pub const CAMPFIRE_LIT: u16 = 63;
const REACH: i32 = 2;

fn distancia(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

impl World {
    /// A click on a floor tile: remembered for the dagger (`flags.TargetObj*`), and an
    /// unlit campfire is lit. Returns whether there was something to act on.
    pub(crate) fn floor_click(&mut self, id: u32, map: u16, x: i32, y: i32) -> bool {
        let Some(item) = self.instance(map).ground.at(x, y) else { return false };
        self.player_mut(id).target_floor = Some((map, x, y));
        if item.obj == CAMPFIRE_UNLIT {
            self.light_campfire(id, map, x, y);
            return true;
        }
        if item.obj == WOOD {
            self.console(id, "Leña. Usá una daga sobre ella para preparar una fogata.", Tone::Info);
            return true;
        }
        false
    }

    /// `TratarDeHacerFogata`: the dagger on the pile of wood clicked last (3 or more logs).
    pub(crate) fn try_make_campfire(&mut self, id: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        let Some((map, x, y)) = p.target_floor.filter(|(m, ..)| *m == p.map) else { return false };
        let Some(item) = self.instance(map).ground.at(x, y).filter(|i| i.obj == WOOD) else { return false };
        if distancia((p.x, p.y), (x, y)) > REACH || self.is_dead(id) || item.amount < 3 {
            return true;
        }
        let skill = p.stats.skills[SKILL_SURVIVAL];
        let luck = match skill {
            0..=5 => 3,
            6..=34 => 2,
            _ => 1,
        };
        if self.rng.range(1, luck) == 1 {
            self.erase_obj(map, x, y, item.amount);
            self.make_obj(map, x, y, InvItem { obj: CAMPFIRE_UNLIT, amount: (item.amount / 3).max(1) });
            self.console(id, "Has preparado una fogata.", Tone::Good);
        } else {
            self.console(id, "No has podido preparar la fogata.", Tone::Info);
        }
        self.raise_skill(id, Skill::Survival);
        true
    }

    /// `HandleCampfireTwigAction`: lighting an unlit campfire.
    fn light_campfire(&mut self, id: u32, map: u16, x: i32, y: i32) {
        let Some(p) = self.player(id) else { return };
        if distancia((p.x, p.y), (x, y)) > REACH {
            return self.console(id, "Estas demasiado lejos.", Tone::Warn);
        }
        let skill = p.stats.skills[SKILL_SURVIVAL];
        let m = &self.instance(map).map;
        if m.rain && self.weather.precipitation {
            return self.console(id, "Está lloviendo, no podés encender una fogata aquí.", Tone::Warn);
        }
        if m.safe || m.trigger(x, y) == ao_formats::csm::TRIGGER_SAFE_ZONE {
            return self.console(id, "En zona segura no podés hacer fogatas.", Tone::Warn);
        }
        let g = &self.instance(map).ground;
        if [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)].iter().any(|&(ax, ay)| g.at(ax, ay).is_some_and(|i| i.obj == CAMPFIRE_LIT)) {
            return self.console(id, "Debes alejarte un poco de la otra fogata.", Tone::Warn);
        }
        let luck = match skill {
            0..=5 => 4,
            6..=9 => 3,
            _ => 2,
        };
        let city = m.zone.eq_ignore_ascii_case("CIUDAD");
        if self.rng.range(1, luck) == 1 {
            if city {
                return self.console(id, "La ley impide realizar fogatas en las ciudades.", Tone::Warn);
            }
            self.erase_obj(map, x, y, u32::MAX);
            self.make_obj(map, x, y, InvItem { obj: CAMPFIRE_LIT, amount: 1 });
            self.console(id, "Has prendido la fogata.", Tone::Good);
        } else {
            self.console(id, "No has podido hacer fuego.", Tone::Info);
        }
        self.raise_skill(id, Skill::Survival);
    }

    /// `ApagarFogatas`: when it starts to rain, lit campfires on rainy maps go out.
    pub(crate) fn put_out_campfires(&mut self) {
        let maps: Vec<u16> = self.maps.iter().filter(|(_, m)| m.map.rain).map(|(n, _)| *n).collect();
        for map in maps {
            let lit: Vec<(i32, i32)> = self.instance(map).ground.tiles.iter().filter(|(_, i)| i.obj == CAMPFIRE_LIT).map(|(k, _)| *k).collect();
            for (x, y) in lit {
                self.erase_obj(map, x, y, u32::MAX);
                self.make_obj(map, x, y, InvItem { obj: CAMPFIRE_UNLIT, amount: 1 });
            }
        }
    }
}
