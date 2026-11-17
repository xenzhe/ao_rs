//! Health, stamina, hunger and thirst, once per second. OWNER: progression system (P1,
//! Sonnet 1).
//!
//! Ported from `General.bas`: `EfectoStamina`, `Sanar`, `RecStamina`, `HambreYSed`
//! (intervals of `intervalos.ini`), and from `Trabajo.DoMeditar` for meditation mana.
//!
//! The VB runs `EfectoStamina` on every 40 ms game tick and `HambreYSed` once per second.
//! The world calls `regen` once per second, so each call replays 25 game ticks with the
//! same counters: the rhythm is the official one (health every ~8 s, stamina every ~0.5 s,
//! thirst every 161 s and hunger every 181 s).
use crate::world::{World, REGEN_MS};
use ao_formats::dat::balance::Class;
use ao_protocol::{Tone, S2C};
use ao_rules::progression::{self as rules, vb_round};

/// `GAME_EVENTS_INTERVAL_MS` (modGameEvents.bas).
const GAME_TICK_MS: u64 = 40;
/// `SanaIntervaloSinDescansar` / `SanaIntervaloDescansar` (game ticks).
const HP_INTERVAL: u32 = 200;
const HP_INTERVAL_RESTING: u32 = 50;
/// `StaminaIntervaloSinDescansar` / `StaminaIntervaloDescansar`.
const STA_INTERVAL: u32 = 10;
const STA_INTERVAL_RESTING: u32 = 5;
/// `IntervaloSed` / `IntervaloHambre` (`intervalos.ini` value / 25), in seconds.
const THIRST_INTERVAL: u32 = 4000 / 25;
const HUNGER_INTERVAL: u32 = 4500 / 25;
/// `IntervaloEnCombate`: seconds without health regeneration after fighting.
const COMBAT_SECS: u32 = 10;

/// What changed for a player during one regeneration step.
#[derive(Default)]
struct Changed {
    hp: bool,
    mana: bool,
    vitals: bool,
}

impl World {
    /// Rolls like VB6 `RandomNumber(lo, hi)`: `Fix(Rnd * (hi - lo + 1)) + lo`.
    fn random_number(&mut self, lo: i32, hi: i32) -> i32 {
        if hi < lo {
            return lo;
        }
        lo + ((self.rng.next_u32() as u64 * (hi - lo + 1) as u64) >> 32) as i32
    }

    /// Called every `REGEN_MS` by the world tick.
    pub(crate) fn regen(&mut self) {
        let legacy_mana = self.data.balance.mana_regen;
        for id in self.player_ids() {
            // The dead neither regenerate nor get hungry (`If .flags.Muerto = 0`).
            if self.player(id).is_none_or(|p| p.combat.dead || p.hp <= 0) {
                continue;
            }
            let mut changed = Changed::default();
            let p = self.player_mut(id);
            p.stats.timers.combat_secs = p.stats.timers.combat_secs.saturating_sub(1);
            self.hunger_and_thirst(id, &mut changed);
            for _ in 0..REGEN_MS / GAME_TICK_MS {
                self.effect_stamina(id, &mut changed);
            }
            // Not official: Balance has no passive mana (only meditation, see `mana_regen_tick`).
            // Kept because `data/balance.toml` `mana_regen` drives it; set it to 0 for the VB rule.
            let p = self.player_mut(id);
            if legacy_mana > 0 && p.mana < p.max_mana {
                p.mana = (p.mana + legacy_mana).min(p.max_mana);
                changed.mana = true;
            }
            // `EfectoStamina`: resting ends by itself when health and stamina are full.
            if p.stats.timers.resting && p.hp == p.max_hp && p.stats.sta == p.stats.max_sta {
                p.stats.timers.resting = false;
                self.console(id, "Has terminado de descansar.", Tone::Info);
            }
            if changed.mana {
                self.send_stats(id);
            }
            if changed.vitals {
                self.send_vitals(id);
            }
        }
    }

    /// `General.HambreYSed`: every 161 s thirst and every 181 s hunger drop by 10.
    fn hunger_and_thirst(&mut self, id: u32, changed: &mut Changed) {
        let st = &mut self.player_mut(id).stats;
        if st.water > 0 {
            if st.timers.thirst < THIRST_INTERVAL {
                st.timers.thirst += 1;
            } else {
                st.timers.thirst = 0;
                st.water = (st.water - 10).max(0);
                changed.vitals = true;
            }
        }
        if st.food > 0 {
            if st.timers.hunger < HUNGER_INTERVAL {
                st.timers.hunger += 1;
            } else {
                st.timers.hunger = 0;
                st.food = (st.food - 10).max(0);
                changed.vitals = true;
            }
        }
    }

    /// `General.EfectoStamina`: one 40 ms game tick of health and stamina regeneration.
    /// Nothing regenerates while hungry or thirsty, nor without armour worn (`Desnudo`);
    /// health also waits while in combat.
    fn effect_stamina(&mut self, id: u32, changed: &mut Changed) {
        let Some(p) = self.player(id) else { return };
        let starving = p.stats.food == 0 || p.stats.water == 0;
        let naked = p.inv.equipped.armor.is_none();
        let (resting, in_combat) = (p.stats.timers.resting, p.stats.timers.combat_secs > 0);
        let (hp, max_hp, sta, max_sta) = (p.hp, p.max_hp, p.stats.sta, p.stats.max_sta);
        let class = p.class;
        let survival = p.stats.skills[crate::world::stats::SKILL_SURVIVAL];

        // Sanar
        if !starving && !in_combat && hp < max_hp && !naked {
            let interval = if resting { HP_INTERVAL_RESTING } else { HP_INTERVAL };
            let p = self.player_mut(id);
            if p.stats.timers.hp < interval {
                p.stats.timers.hp += 1;
            } else {
                p.stats.timers.hp = 0;
                let lo = vb_round(max_hp as f64 * 5.0 / 100.0);
                let hi = vb_round(max_hp as f64 * 10.0 / 100.0);
                let amount = self.random_number(lo, hi);
                let p = self.player_mut(id);
                p.hp = (p.hp + amount).min(p.max_hp);
                let (map, hp, max_hp) = (p.map, p.hp, p.max_hp);
                self.console(id, "Has sanado.", Tone::Good);
                self.broadcast(map, &S2C::Hp { id, hp, max_hp }, None);
                changed.hp = true;
                self.send_stats(id);
            }
        }

        // RecStamina
        if !naked && !starving && sta < max_sta {
            let interval = if resting { STA_INTERVAL_RESTING } else { STA_INTERVAL };
            let p = self.player_mut(id);
            if p.stats.timers.sta < interval {
                p.stats.timers.sta += 1;
            } else {
                p.stats.timers.sta = 0;
                let luck = match survival {
                    0..=10 => 5,
                    100.. => 25,
                    s => 5 + 2 * ((s as i32 - 1) / 10),
                };
                let share = vb_round(max_sta as f64 * luck as f64 / 100.0);
                let top = if class == Class::Worker.id() { share } else { vb_round(share as f64 / 1.6) };
                let gain = self.random_number(1, top);
                let p = self.player_mut(id);
                p.stats.sta = (p.stats.sta + gain).min(p.stats.max_sta);
                changed.vitals = true;
            }
        }
    }

    /// Marks a player as fighting for 10 s (`Counters.EnCombate = IntervaloEnCombate`): no
    /// health regeneration meanwhile. For the combat system to call when it hits or is hit.
    #[allow(dead_code)]
    pub(crate) fn mark_in_combat(&mut self, id: u32) {
        if let Some(s) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) {
            s.stats.timers.combat_secs = COMBAT_SECS;
        }
    }

    /// One meditation step (`Trabajo.DoMeditar`, without lutes): adds mana by the official
    /// formula and tells the client. Returns true when mana is full, which is when the
    /// meditation ends. The magic system owns the toggle and the timing (every
    /// `IntervaloMeditar` ticks, after the first 20) and calls this each time it is due.
    #[allow(dead_code)]
    pub(crate) fn mana_regen_tick(&mut self, id: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        let amount = rules::meditation_mana(&self.ao.balance, p.max_mana, p.stats.skills[crate::world::stats::SKILL_MEDITATE]);
        let p = self.player_mut(id);
        let full = p.mana + amount >= p.max_mana;
        p.mana = if full { p.max_mana } else { p.mana + amount };
        self.send_stats(id);
        full
    }
}

#[cfg(test)]
mod tests {
    use crate::systems::progression::testkit::Bench;
    use ao_protocol::S2C;

    fn bench() -> Option<Bench> {
        ao_formats::paths::recursos_dir().join("Dat").is_dir().then(Bench::new)
    }

    /// Dressed (armour worn), hurt, well fed: regenerates.
    fn hurt(b: &mut Bench) {
        let id = b.id;
        let p = b.world.player_mut(id);
        p.inv.equipped.armor = Some(1);
        p.max_hp = 100;
        p.hp = 1;
        p.stats.food = 100;
        p.stats.water = 100;
    }

    fn seconds(b: &mut Bench, n: u32) {
        for _ in 0..n {
            b.world.regen();
        }
    }

    #[test]
    fn thirst_and_hunger_fall_at_their_intervals() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        seconds(&mut b, 160);
        let p = b.world.player(id).unwrap();
        assert_eq!((p.stats.water, p.stats.food), (100, 100), "nothing yet after 160 s");
        seconds(&mut b, 1);
        let p = b.world.player(id).unwrap();
        assert_eq!((p.stats.water, p.stats.food), (90, 100), "thirst -10 on second 161");
        assert!(b.drain().iter().any(|m| matches!(m, S2C::Vitals { water: 90, food: 100, .. })));
        seconds(&mut b, 20);
        let p = b.world.player(id).unwrap();
        assert_eq!((p.stats.water, p.stats.food), (90, 90), "hunger -10 on second 181");
        // Never below zero.
        b.world.player_mut(id).stats.water = 5;
        seconds(&mut b, 161);
        assert_eq!(b.world.player(id).unwrap().stats.water, 0);
        seconds(&mut b, 400);
        assert_eq!(b.world.player(id).unwrap().stats.water, 0);
    }

    #[test]
    fn health_regenerates_only_when_fed_dressed_and_out_of_combat() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        // Positive control: after 9 s (225 ticks) one heal of 5-10% of 100 has happened.
        hurt(&mut b);
        seconds(&mut b, 9);
        let hp = b.world.player(id).unwrap().hp;
        assert!((6..=11).contains(&hp), "healed once: {hp}");
        assert!(b.drain().iter().any(|m| matches!(m, S2C::Hp { .. })));

        // Thirsty: nothing, however long we wait.
        hurt(&mut b);
        b.world.player_mut(id).stats.water = 0;
        seconds(&mut b, 30);
        assert_eq!(b.world.player(id).unwrap().hp, 1, "no regeneration without water");
        hurt(&mut b);
        b.world.player_mut(id).stats.food = 0;
        seconds(&mut b, 30);
        assert_eq!(b.world.player(id).unwrap().hp, 1, "no regeneration without food");

        // Naked (no armour): nothing.
        hurt(&mut b);
        b.world.player_mut(id).inv.equipped.armor = None;
        seconds(&mut b, 30);
        assert_eq!(b.world.player(id).unwrap().hp, 1, "naked characters do not regenerate");

        // In combat: nothing for 10 s, then the usual ~8 s.
        hurt(&mut b);
        b.world.mark_in_combat(id);
        seconds(&mut b, 10);
        assert_eq!(b.world.player(id).unwrap().hp, 1);
        seconds(&mut b, 9);
        assert!(b.world.player(id).unwrap().hp > 1, "heals once the combat timer ends");

        // The dead do not regenerate.
        hurt(&mut b);
        b.world.player_mut(id).combat.dead = true;
        b.world.player_mut(id).hp = 0;
        seconds(&mut b, 30);
        assert_eq!(b.world.player(id).unwrap().hp, 0);
    }

    #[test]
    fn resting_regenerates_faster() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        hurt(&mut b);
        b.world.player_mut(id).stats.timers.resting = true;
        seconds(&mut b, 3); // 75 ticks > 51
        assert!(b.world.player(id).unwrap().hp > 1, "resting heals in about 2 s");
        // Fully recovered: resting stops by itself.
        let p = b.world.player_mut(id);
        p.hp = p.max_hp;
        p.stats.sta = p.stats.max_sta;
        seconds(&mut b, 1);
        assert!(!b.world.player(id).unwrap().stats.timers.resting);
    }

    #[test]
    fn stamina_recovers_with_survival_and_stops_when_starving() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        hurt(&mut b);
        let p = b.world.player_mut(id);
        p.class = 3;
        p.stats.max_sta = 60;
        p.stats.sta = 0;
        // Survival 0: 5% of 60 = 3, / 1.6 = 1.875 -> 2, so 1-2 per step, two steps per second.
        seconds(&mut b, 1);
        let sta = b.world.player(id).unwrap().stats.sta;
        assert!((2..=4).contains(&sta), "two steps of 1-2: {sta}");
        assert!(b.drain().iter().any(|m| matches!(m, S2C::Vitals { max_sta: 60, .. })));
        // Fills up and stays capped.
        seconds(&mut b, 60);
        assert_eq!(b.world.player(id).unwrap().stats.sta, 60);
        // Hungry: no recovery.
        let p = b.world.player_mut(id);
        p.stats.sta = 0;
        p.stats.food = 0;
        seconds(&mut b, 10);
        assert_eq!(b.world.player(id).unwrap().stats.sta, 0);
        // A worker recovers more per step (no /1.6): 1-3.
        let p = b.world.player_mut(id);
        p.class = 9;
        p.stats.food = 100;
        seconds(&mut b, 1);
        assert!(b.world.player(id).unwrap().stats.sta >= 2);
    }

    #[test]
    fn meditation_step_and_legacy_mana() {
        let Some(mut b) = bench() else { return };
        let id = b.id;
        let p = b.world.player_mut(id);
        p.max_mana = 150;
        p.mana = 100;
        // 7% of (50 + 0)% of 150 = 5.25 -> 5 per step.
        assert!(!b.world.mana_regen_tick(id));
        assert_eq!(b.world.player(id).unwrap().mana, 105);
        b.world.player_mut(id).mana = 148;
        assert!(b.world.mana_regen_tick(id), "full: meditation ends");
        assert_eq!(b.world.player(id).unwrap().mana, 150);
        // The configured passive regeneration still runs once per second.
        let per_second = b.world.data.balance.mana_regen;
        b.world.player_mut(id).mana = 10;
        seconds(&mut b, 2);
        assert_eq!(b.world.player(id).unwrap().mana, 10 + 2 * per_second);
    }
}
