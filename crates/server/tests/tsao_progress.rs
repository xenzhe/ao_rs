//! TSAO progression over the wire with the converted Tierras Sagradas data (AO_TSAO, else
//! X:\dev\tsao_rec; skipped without it, fails if AO_TSAO is set and wrong): points from the
//! level milestones, the class bonus of level 53 (forged and repeated answers refused), the
//! three exchanges (tournament prizes, TS shop, donation packs; forged and unaffordable
//! requests refused, nothing lost), the popular vote (one per account), marriage, and all of
//! it persisted across a logout.
use ao_protocol::p4::tsao::TsaoC2S;
use ao_protocol::C2S;
use ao_testkit::{Bot, TestServer};
use std::path::PathBuf;

const PASS: &str = "clave-segura-1";
/// Tanaris (map 28), open street.
const SPOT: (u16, i32, i32) = (28, 64, 50);

fn tsao() -> Option<PathBuf> {
    match std::env::var("AO_TSAO") {
        Ok(p) => {
            let p = PathBuf::from(p);
            assert!(p.join("Dat").join("ClassBonus.dat").exists(), "AO_TSAO={} no tiene los datos de progreso (tools/tsao/build.cmd)", p.display());
            Some(p)
        }
        Err(_) => Some(PathBuf::from("X:/dev/tsao_rec")).filter(|p| p.join("Dat").join("ClassBonus.dat").exists()),
    }
}

fn server(rec: PathBuf) -> TestServer {
    TestServer::with(|st| {
        st.recursos = rec;
        st.start = Some(SPOT);
        (st.data.balance.start_map, st.data.balance.start_x, st.data.balance.start_y) = SPOT;
    })
}

fn step(s: &TestServer, b: &mut Bot) {
    s.advance(50);
    b.sync();
}

fn says(b: &Bot, t: &str) -> bool {
    b.console().any(|l| l.contains(t))
}

fn points(b: &Bot) -> (u32, u32, u32) {
    let p = b.game.local.p4.tsao.points;
    (p.tournament, p.ts, p.credits)
}

fn count(b: &Bot, obj: u16) -> u32 {
    b.game.local.inventory.iter().flatten().filter(|i| i.obj == obj).map(|i| i.amount).sum()
}

#[test]
fn data_is_converted() {
    let Some(rec) = tsao() else { return eprintln!("TSAO data not found, skipping") };
    let d = ao_formats::dat::tsao_progress::load(&rec);
    assert_eq!(d.prizes.len(), 27);
    assert_eq!(d.ts_shop.len(), 11);
    assert_eq!(d.bonus.levels, vec![53, 56, 60]);
    assert_eq!(d.bonus.classes.len(), 8, "the eight TSAO classes");
    assert!(d.packs.iter().any(|p| !p.rewards.iter().all(|r| r.supported())), "packs with premium/skins are kept but not sold");
    assert_eq!(d.poll.as_ref().map(|p| p.options.len()), Some(4));
    // QUESTS.DAT quest 2: 25 users, 5 tournament points.
    let q = ao_formats::dat::quests::load(&rec).unwrap();
    let q2 = q.data.get(2).unwrap();
    assert_eq!((q2.reward_tournament, q2.reward_ts, q2.reward_credits), (5, 0, 0));
    // GivePTS of the hostile NPCs.
    let npcs = ao_formats::dat::npcs::load(&rec).unwrap();
    assert!(npcs.data.iter().any(|(_, n)| n.give_points == 200));
}

#[test]
fn points_bonus_exchange_vote_and_marriage() {
    let Some(rec) = tsao() else { return eprintln!("TSAO data not found, skipping") };
    let data = ao_formats::dat::tsao_progress::load(&rec);
    let s = server(rec);
    let mut a = s.connect();
    assert!(a.create_account("cuenta_p", PASS));
    // Human mage.
    assert!(a.create_character("Puntero", 1, 1, 1, 5));
    assert!(a.create_character("Segundo", 1, 2, 1, 5));
    a.play("Puntero");
    step(&s, &mut a);
    assert_eq!(points(&a), (0, 0, 0));

    // Level 1 -> 70: milestones 50 (50 pts), 60 (200), 65 (2 TS), 70 (200 + 5 TS); the
    // bonus tiers are offered one after the other, the first one first.
    a.say("/exp 2000000000");
    a.wait_until("bonus ofrecido", |b| b.game.local.p4.tsao.bonus_offer.is_some());
    step(&s, &mut a);
    assert_eq!(a.level(), 70);
    assert_eq!(points(&a), (450, 7, 0), "level milestones");
    let (tier, level, options) = a.game.local.p4.tsao.bonus_offer.clone().unwrap();
    assert_eq!((tier, level), (0, 53));
    let mage = data.bonus.options("mago", 0).unwrap();
    assert_eq!(options, [mage[0].text.clone(), mage[1].text.clone()]);

    // A forged tier is refused; option 1 of the mage (+3 HP) is taken once.
    a.send(&C2S::Tsao(TsaoC2S::ChooseBonus { tier: 2, option: 1 }));
    step(&s, &mut a);
    assert!(a.game.local.p4.tsao.bonuses.is_empty());
    let hp0 = a.game.stats.max_hp.get();
    a.send(&C2S::Tsao(TsaoC2S::ChooseBonus { tier: 0, option: 1 }));
    a.send(&C2S::Tsao(TsaoC2S::ChooseBonus { tier: 0, option: 1 }));
    a.wait_until("bonus elegido", |b| b.game.local.p4.tsao.bonuses.len() == 1);
    step(&s, &mut a);
    assert_eq!(a.game.local.p4.tsao.bonuses, vec![mage[1].text.clone()]);
    assert_eq!(a.game.stats.max_hp.get(), hp0 + mage[1].amount, "+HP once");
    a.wait_until("segundo bonus", |b| b.game.local.p4.tsao.bonus_offer.as_ref().is_some_and(|o| o.0 == 1 && o.1 == 56));

    // Exchange: tournament prize, TS item, donation pack.
    a.say("/darpuntos 600 13 100");
    a.wait_until("puntos", |b| points(b) == (1050, 20, 100));
    a.say("/canje");
    a.wait_until("canje", |b| b.game.local.p4.tsao.exchange.is_some());
    let ex = a.game.local.p4.tsao.exchange.clone().unwrap();
    assert_eq!(ex.prizes.len(), data.prizes.len());
    let (zi, z) = ex.prizes.iter().enumerate().find(|(_, z)| z.price * 2 <= 1050).expect("an affordable prize");
    let zi = zi as u16;
    // Forged amounts and indexes take nothing.
    for m in [TsaoC2S::BuyPrize { index: zi, amount: 0 }, TsaoC2S::BuyPrize { index: zi, amount: 4_000_000 }, TsaoC2S::BuyPrize { index: 999, amount: 1 }, TsaoC2S::BuyPack { index: 999 }] {
        a.send(&C2S::Tsao(m));
    }
    step(&s, &mut a);
    assert_eq!(points(&a), (1050, 20, 100));
    a.send(&C2S::Tsao(TsaoC2S::BuyPrize { index: zi, amount: 2 }));
    a.wait_until("premio", |b| count(b, z.obj) == 2);
    step(&s, &mut a);
    assert_eq!(points(&a).0, 1050 - 2 * z.price);
    let (ti, t) = ex.ts.iter().enumerate().find(|(_, t)| t.price <= 20).unwrap();
    let before = count(&a, t.obj);
    a.send(&C2S::Tsao(TsaoC2S::BuyTs { index: ti as u16 }));
    a.wait_until("objeto TS", |b| count(b, t.obj) == before + t.amount);
    step(&s, &mut a);
    assert_eq!(points(&a).1, 20 - t.price);
    // A pack with premium is not sold; an affordable item pack is.
    if let Some((bad, _)) = ex.packs.iter().enumerate().find(|(_, p)| !p.available) {
        a.send(&C2S::Tsao(TsaoC2S::BuyPack { index: bad as u16 }));
        a.wait_until("pack no disponible", |b| says(b, "no se puede canjear"));
    }
    let (pi, pk) = ex.packs.iter().enumerate().find(|(_, p)| p.available && p.price <= 100).expect("an affordable pack");
    let pack_obj = data.packs[pi].rewards.iter().find_map(|r| if let ao_formats::dat::tsao_progress::PackReward::Item { obj, .. } = r { Some(*obj) } else { None }).unwrap();
    let before = count(&a, pack_obj);
    a.send(&C2S::Tsao(TsaoC2S::BuyPack { index: pi as u16 }));
    a.wait_until("pack", |b| count(b, pack_obj) > before);
    step(&s, &mut a);
    assert_eq!(points(&a).2, 100 - pk.price);
    // Not enough credits: refused, nothing taken.
    let pts = points(&a);
    let (big, _) = ex.packs.iter().enumerate().find(|(_, p)| p.available && p.price > pts.2).unwrap();
    a.send(&C2S::Tsao(TsaoC2S::BuyPack { index: big as u16 }));
    a.wait_until("sin creditos", |b| says(b, "Necesitas"));
    step(&s, &mut a);
    assert_eq!(points(&a), pts);

    // Vote: once per account (the second character of the same account cannot).
    a.say("/votar");
    a.wait_until("votacion", |b| b.game.local.p4.tsao.poll.is_some());
    assert!(a.game.local.p4.tsao.poll.as_ref().unwrap().results.is_none());
    a.send(&C2S::Tsao(TsaoC2S::Vote { option: 9 }));
    a.send(&C2S::Tsao(TsaoC2S::Vote { option: 1 }));
    a.wait_until("resultados", |b| b.game.local.p4.tsao.poll.as_ref().is_some_and(|p| p.results.is_some()));
    assert_eq!(a.game.local.p4.tsao.poll.as_ref().unwrap().results.as_ref().unwrap()[1], 1);

    // Marriage with a player of another account.
    let mut b = s.connect();
    assert!(b.create_account("cuenta_q", PASS));
    assert!(b.create_character("Novia", 1, 2, 2, 70));
    b.play("Novia");
    step(&s, &mut b);
    b.say("/casar Puntero");
    a.wait_until("propuesta", |b| says(b, "te ha pedido matrimonio"));
    a.say("/casar Novia");
    b.wait_until("boda", |b| says(b, "se han casado"));
    // Consultas.dat: level 10 to vote.
    b.say("/exp 200000");
    b.wait_until("nivel 10", |b| b.level() >= 10);
    b.say("/votar");
    b.wait_until("votacion b", |b| b.game.local.p4.tsao.poll.is_some());
    b.send(&C2S::Tsao(TsaoC2S::Vote { option: 0 }));
    b.wait_until("voto b", |b| b.game.local.p4.tsao.poll.as_ref().is_some_and(|p| p.results.is_some()));
    // Bot b voting must not count for a's account and vice versa.
    assert_eq!(b.game.local.p4.tsao.poll.as_ref().unwrap().results.as_ref().unwrap(), &vec![1, 1, 0, 0]);

    // Logout and back: points, bonus, marriage kept; tier 0 not offered again.
    let pts = points(&a);
    drop(a);
    s.advance(200);
    let mut a = s.connect();
    assert!(a.account_login("cuenta_p", PASS));
    a.play("Puntero");
    a.wait_until("puntos tras volver", |b| points(b) == pts);
    step(&s, &mut a);
    assert_eq!(a.game.local.p4.tsao.bonuses.len(), 1);
    a.wait_until("pendiente otra vez", |b| b.game.local.p4.tsao.bonus_offer.as_ref().is_some_and(|o| o.0 == 1));
    a.say("/puntos");
    a.wait_until("casado", |b| says(b, "Casado/a con Novia"));
    // The same account cannot vote again with another character.
    drop(a);
    s.advance(200);
    let mut c = s.connect();
    assert!(c.account_login("cuenta_p", PASS));
    c.play("Segundo");
    step(&s, &mut c);
    c.say("/exp 200000");
    c.wait_until("nivel 10", |b| b.level() >= 10);
    c.send(&C2S::Tsao(TsaoC2S::Vote { option: 2 }));
    c.wait_until("ya voto", |b| says(b, "ya votó"));
    // Divorce from b while a is offline: both sides go.
    b.say("/divorciarse");
    b.wait_until("divorcio", |b| says(b, "se han divorciado"));
    drop(c);
    s.advance(200);
    let mut a = s.connect();
    assert!(a.account_login("cuenta_p", PASS));
    a.play("Puntero");
    step(&s, &mut a);
    a.say("/puntos");
    a.wait_until("puntos", |b| says(b, "Puntos de torneo"));
    assert!(!says(&a, "Casado/a"));
}
