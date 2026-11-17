//! P3 server pieces the client needs: the world weather (ModClimas) reaches everyone, and a
//! looping FX already playing (meditation) is sent to whoever arrives later.
use ao_protocol::{C2S, S2C};
use ao_testkit::TestServer;

fn weathers(log: &[S2C]) -> Vec<(u8, bool)> {
    log.iter().filter_map(|m| if let S2C::Weather { fog, precipitation } = m { Some((*fog, *precipitation)) } else { None }).collect()
}

#[test]
fn weather_is_sent_on_entering_and_on_every_change() {
    let s = TestServer::start();
    let mut ana = s.bot("Ana");
    ana.sync();
    assert_eq!(weathers(ana.log()).first(), Some(&(0, false)), "clear sky on entering");
    assert_eq!(ana.game.local.weather, (0, false));

    // Three full 20-minute cycles, a game minute at a time.
    for _ in 0..60 {
        s.advance(60_000);
        ana.sync();
    }
    let seen = weathers(ana.log());
    // Every change is a valid state, and every cycle ends clear.
    assert!(seen.iter().all(|(fog, p)| (*fog == 0 || (10..=45).contains(fog)) && (!*p || *fog > 0)), "{seen:?}");
    assert_eq!(ana.game.local.weather, *seen.last().unwrap());
    let thunders = ana.log().iter().filter(|m| matches!(m, S2C::Thunder)).count();
    assert_eq!(thunders, seen.iter().filter(|(_, p)| *p).count(), "a thunder each time it starts raining");
}

#[test]
fn meditation_is_seen_by_who_arrives_later() {
    let s = TestServer::start();
    let mut ana = s.bot("Ana");
    ana.wait_until("libro de hechizos", |b| !b.game.spells.is_empty() && b.mana() > 0);
    // Nobody meditates at full mana: spend some first (Invisibilidad Menor on herself).
    let (x, y) = ana.pos();
    assert!(ana.cast(24, x, y));
    ana.sync();
    ana.send(&C2S::Meditate);
    ana.sync();
    let aid = ana.id();
    assert!(ana.log().iter().any(|m| matches!(m, S2C::Fx { id, loops: 255, .. } if *id == aid)), "Ana meditates");

    let mut beto = s.bot("Beto");
    beto.sync();
    assert!(beto.log().iter().any(|m| matches!(m, S2C::Fx { id, fx, loops: 255 } if *id == aid && *fx != 0)), "Beto sees Ana's meditation on arrival");
}
