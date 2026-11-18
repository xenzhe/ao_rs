//! World weather, port of `ModClimas.bas` + `frmMain.TimerMeteorologia_Timer`: a 20-minute
//! cycle ticking once per (game) minute. At 11 minutes left, 1 in 2 the sky clouds over
//! (intensity 10-45); at 6 left, if cloudy, 2 in 5 it starts to rain/snow (with a thunder);
//! when the cycle ends everything clears. Everyone gets the changes; each player gets the
//! current state on entering the world.
use crate::clock::GameTime;
use crate::world::World;
use ao_protocol::S2C;

/// `METEOROLOGICAL_CYCLE_MINUTES`, `..._CLOUD_COUNTDOWN_MINUTES`, `..._PRECIPITATION_...`.
pub const CYCLE_MINUTES: u8 = 20;
pub const CLOUD_AT: u8 = 11;
pub const PRECIPITATION_AT: u8 = 6;
const MINUTE_MS: u64 = 60_000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Weather {
    /// `TimerMeteorologico`: minutes left in the cycle.
    pub countdown: u8,
    /// `IntensidadDeNubes` (0 = clear sky).
    pub fog: u8,
    /// `Lloviendo` / `Nebando`.
    pub precipitation: bool,
    pub next_tick: GameTime,
}

impl Weather {
    pub fn message(&self) -> S2C {
        S2C::Weather { fog: self.fog, precipitation: self.precipitation }
    }
}

impl World {
    /// Called every world tick; acts once per game minute.
    pub(crate) fn tick_weather(&mut self, now: GameTime) {
        if self.weather.next_tick == GameTime::ZERO {
            self.weather.next_tick = now.after_ms(MINUTE_MS);
            self.weather.countdown = CYCLE_MINUTES;
            return;
        }
        if now < self.weather.next_tick {
            return;
        }
        self.weather.next_tick = now.after_ms(MINUTE_MS);
        let w = &mut self.weather;
        if w.countdown == 0 || w.countdown > CYCLE_MINUTES {
            w.countdown = CYCLE_MINUTES;
        }
        let before = (w.fog, w.precipitation);
        let mut thunder = false;
        if w.countdown == CLOUD_AT {
            // WeatherCloudRollSucceeds: RandomNumber(1, 2) = 1.
            if self.rng.range(1, 2) == 1 {
                self.weather.fog = self.rng.range(10, 45) as u8;
            }
        } else if w.countdown == PRECIPITATION_AT && w.fog > 0 {
            // WeatherPrecipitationRollSucceeds: RandomNumber(1, 5) in 1..=2.
            if self.rng.range(1, 5) <= 2 {
                self.weather.precipitation = true;
                thunder = true;
            }
        }
        let w = &mut self.weather;
        if w.countdown == 1 {
            // ResetMeteo
            *w = Weather { countdown: CYCLE_MINUTES, next_tick: w.next_tick, ..Weather::default() };
        } else {
            w.countdown -= 1;
        }
        if thunder {
            self.broadcast_all(&S2C::Thunder, None);
        }
        if (self.weather.fog, self.weather.precipitation) != before {
            let msg = self.weather.message();
            self.broadcast_all(&msg, None);
        }
    }

    /// The current weather to a player entering the world (`DetermineInitialWeatherSynchronization`).
    /// The world clock a player gets on entering (`WriteHora`).
    pub(crate) fn world_time_message(&self) -> S2C {
        let day_ms = (self.cfg.world.day_length_minutes.max(1)) * 60_000;
        // The day starts at 08:00 when the server starts (the official one, at 00:00, puts
        // every restart in the dark).
        S2C::WorldTime { elapsed_ms: (self.now().0 + day_ms / 3) % day_ms, day_ms }
    }

    pub(crate) fn weather_on_enter(&mut self, id: u32) {
        let time = self.world_time_message();
        self.send(id, &time);
        let msg = self.weather.message();
        self.send(id, &msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Clock, VirtualClock};
    use crate::Settings;

    #[test]
    fn a_cycle_clouds_maybe_rains_and_clears() {
        // Over many seeds: clouds only from minute 9 (11 left), rain only when cloudy and
        // from minute 14 (6 left), and everything clear after the 20th minute.
        let mut clouded = 0;
        let mut rained = 0;
        for seed in 0..40 {
            let clock = VirtualClock::new();
            let mut st = Settings::defaults();
            st.seed = seed;
            st.clock = Clock::Virtual(clock.clone());
            let mut w = World::new(st);
            w.tick_weather(w.now());
            let mut saw_fog = false;
            for minute in 1..=CYCLE_MINUTES as u64 {
                clock.set(GameTime(minute * MINUTE_MS));
                w.tick_weather(w.now());
                let s = w.weather;
                if minute < (CYCLE_MINUTES - CLOUD_AT + 1) as u64 {
                    assert_eq!(s.fog, 0, "seed {seed} minute {minute}");
                }
                if s.precipitation {
                    assert!(s.fog > 0 && minute >= (CYCLE_MINUTES - PRECIPITATION_AT + 1) as u64);
                    rained += 1;
                }
                if (10..=45).contains(&s.fog) {
                    saw_fog = true;
                }
                if minute == CYCLE_MINUTES as u64 {
                    assert_eq!((s.fog, s.precipitation), (0, false), "cleared at the end");
                }
            }
            clouded += saw_fog as u32;
        }
        assert!(clouded > 5 && clouded < 35, "about half the cycles cloud over: {clouded}/40");
        assert!(rained > 0, "some cycles rain");
    }
}
