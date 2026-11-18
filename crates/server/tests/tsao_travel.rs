//! The TSAO transporter (Viajero) over the wire with the converted Tierras Sagradas data
//! (`tools/tsao/build.cmd` -> X:\dev\tsao_rec; skipped when it is not there): the offer,
//! no gold, level-locked city, a paid trip, a far or forged request.
use ao_protocol::p4::travel::TravelC2S;
use ao_protocol::C2S;
use ao_testkit::TestServer;
use std::path::PathBuf;

const GOLD_OBJ: u16 = 12;
/// Tanaris (map 28): the Viajero stands at (48, 51).
const VIAJERO: (i32, i32) = (48, 51);

fn tsao() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var("AO_TSAO").unwrap_or_else(|_| "X:/dev/tsao_rec".into()));
    p.join("Mapas").join("mapa28.csm").exists().then_some(p)
}

#[test]
fn viajero_charges_before_the_trip_and_checks_level() {
    let Some(rec) = tsao() else { return eprintln!("TSAO data not found, skipping") };
    let s = TestServer::with(|st| {
        st.recursos = rec;
        st.start = Some((28, VIAJERO.0, VIAJERO.1 + 2));
    });
    let mut b = s.bot("Viajante");
    b.send(&C2S::Interact { tx: VIAJERO.0, ty: VIAJERO.1 });
    b.wait_until("travel offer", |b| b.game.local.p4.travel.open.is_some());
    let (npc, _, dests) = b.game.local.p4.travel.open.clone().unwrap();
    assert_eq!(dests.len(), 8);
    let anvil = dests.iter().position(|d| d.name == "Anvilmar").unwrap() as u8;
    let inthak = dests.iter().position(|d| d.name == "Inthak").unwrap() as u8;
    assert_eq!(dests[anvil as usize].price, 1000);

    // No gold: refused, still in Tanaris (TSAO let Anvilmar and Kahlimdor through for free).
    b.send(&C2S::Travel(TravelC2S::Go { npc, index: anvil }));
    s.advance(50);
    b.sync();
    assert_eq!(b.map(), 28);

    b.say(&format!("/dar {GOLD_OBJ} 6000"));
    s.advance(50);
    b.sync();
    let gold = b.game.local.gold;
    assert!(gold >= 6000);

    // Level 30 city at level 1: refused, gold kept.
    b.send(&C2S::Travel(TravelC2S::Go { npc, index: inthak }));
    s.advance(50);
    b.sync();
    assert_eq!((b.map(), b.game.local.gold), (28, gold));

    // Forged index: nothing happens.
    b.send(&C2S::Travel(TravelC2S::Go { npc, index: 200 }));
    // Paid trip.
    b.send(&C2S::Travel(TravelC2S::Go { npc, index: anvil }));
    b.wait_until("arrive in Anvilmar", |b| b.map() == 29);
    b.sync();
    assert_eq!(b.game.local.gold, gold - 1000);
    assert!(b.game.local.p4.travel.open.is_none());

    // Far from any Viajero now: the old npc id is not on this map.
    b.send(&C2S::Travel(TravelC2S::Go { npc, index: anvil }));
    s.advance(50);
    b.sync();
    assert_eq!(b.game.local.gold, gold - 1000);
}
