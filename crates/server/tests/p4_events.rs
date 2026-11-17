//! P4 events over the wire: duels (retos) with a gold bet in the official rings of
//! Retos.dat, the auction house and the GM tournament sign-up, with the cheats a client can
//! try (accepting a duel that does not exist or twice, betting gold it does not have,
//! moving gold during the duel, bidding low or on its own lot, disconnecting halfway) and
//! no gold duplicated or lost. Whole best-of-three duels with kills are in the in-crate
//! tests of `systems/event.rs`.
use ao_formats::dat::obj;
use ao_formats::recursos_dir;
use ao_protocol::p4::event::TournamentSpec;
use ao_protocol::{EventC2S, C2S};
use ao_testkit::{Bot, TestServer};

const GOLD_OBJ: u16 = 12;
const BET: u32 = 10_000;
const ARENAS: [u16; 4] = [324, 372, 389, 390];

fn ev(bot: &mut Bot, m: EventC2S) {
    bot.send(&C2S::Event(m));
}

fn gold(bot: &Bot) -> u32 {
    bot.game.local.gold
}

fn give_gold(s: &TestServer, bot: &mut Bot, amount: u32) {
    let before = gold(bot);
    // `/dar` gives at most one stack (10 000) per line.
    let mut left = amount;
    while left > 0 {
        let chunk = left.min(10_000);
        bot.say(&format!("/dar {GOLD_OBJ} {chunk}"));
        left -= chunk;
    }
    s.advance(50);
    bot.sync();
    assert_eq!(gold(bot), before + amount);
}

fn step(s: &TestServer, bots: &mut [&mut Bot]) {
    s.advance(50);
    for b in bots.iter_mut() {
        b.sync();
    }
}

fn said(bot: &Bot, text: &str) -> bool {
    bot.console().any(|l| l.contains(text))
}

fn propose(bot: &mut Bot, players: &[&str], bet: u32) {
    ev(bot, EventC2S::DuelPropose { players: players.iter().map(|s| s.to_string()).collect(), bet, max_potions: -1 });
}

fn accept(bot: &mut Bot, offerer: &str) {
    ev(bot, EventC2S::DuelAccept { offerer: offerer.into() });
}

fn in_duel(bot: &Bot) -> bool {
    bot.game.local.p4.event.view.duel.is_some()
}

/// An object of obj.dat that can be auctioned and stacked.
fn auction_obj() -> u16 {
    let objs = obj::load(&recursos_dir()).unwrap().data;
    (1..2000u16).find(|i| *i != GOLD_OBJ && objs.get(*i as u32).is_some_and(|o| o.auctionable == 1 && !o.name.is_empty())).expect("an auctionable object")
}

#[test]
fn duel_invitations_refuse_the_cheats() {
    let s = TestServer::start();
    let mut a = s.bot("Retador");
    let mut b = s.bot("Rival");
    let mut c = s.bot("Miron");
    give_gold(&s, &mut a, 50_000);
    give_gold(&s, &mut b, 50_000);
    let (ga, gb) = (gold(&a), gold(&b));

    // Accepting a duel that does not exist: unknown offerer, and one that invited nobody.
    accept(&mut b, "Nadie");
    accept(&mut b, "Miron");
    step(&s, &mut [&mut a, &mut b, &mut c]);
    assert!(said(&b, "Nadie no está conectado"));
    assert!(said(&b, "no te ha invitado a ningún reto"));
    assert!(!in_duel(&b));

    // Betting more than you have, or below the minimum of Retos.dat (10 000).
    propose(&mut a, &["Rival"], ga + 1);
    propose(&mut c, &["Rival"], BET);
    propose(&mut a, &["Rival"], 9_999);
    propose(&mut a, &["Retador"], BET);
    propose(&mut a, &["Rival", "Rival", "Miron"], BET);
    step(&s, &mut [&mut a, &mut b, &mut c]);
    assert!(said(&a, "No tienes suficiente oro"));
    assert!(said(&a, "La apuesta mínima es de 10000"));
    assert!(said(&a, "Retador no puede jugar un reto"), "yourself");
    assert!(said(&a, "Rival no puede jugar un reto"), "the same name twice");
    assert!(b.game.local.p4.event.view.invites.iter().all(|i| i.offerer != "Retador"));

    // Miron has no gold: the invitation exists but accepting it is refused.
    assert!(said(&c, "No tienes suficiente oro"));
    propose(&mut a, &["Miron"], BET);
    step(&s, &mut [&mut a, &mut b, &mut c]);
    assert_eq!(c.game.local.p4.event.view.invites.len(), 1);
    accept(&mut c, "Retador");
    step(&s, &mut [&mut a, &mut b, &mut c]);
    assert!(said(&c, "Necesitas al menos 10000 monedas"));
    assert!(!in_duel(&a) && !in_duel(&c));

    // A new request replaces the old one; the old guest can no longer accept it.
    propose(&mut a, &["Rival"], BET);
    step(&s, &mut [&mut a, &mut b, &mut c]);
    assert!(c.game.local.p4.event.view.invites.is_empty());
    accept(&mut c, "Retador");
    step(&s, &mut [&mut a, &mut b, &mut c]);
    assert!(said(&c, "no te ha invitado"));

    // Nothing is taken until the duel starts.
    assert_eq!((gold(&a), gold(&b)), (ga, gb));
    assert_eq!(a.game.local.p4.event.view.pending.as_ref().map(|p| p.missing.clone()), Some(vec!["Rival".to_string()]));

    // Cancelling: the guest can no longer accept.
    ev(&mut a, EventC2S::DuelCancel);
    step(&s, &mut [&mut a, &mut b, &mut c]);
    assert!(said(&b, "El reto ha sido cancelado"));
    accept(&mut b, "Retador");
    step(&s, &mut [&mut a, &mut b, &mut c]);
    assert!(!in_duel(&a));
    assert_eq!((gold(&a), gold(&b)), (ga, gb));
}

#[test]
fn a_2v2_where_one_accepts_twice_and_then_everybody_is_in() {
    let s = TestServer::start();
    let names = ["Uno", "Dos", "Tres", "Cuatro"];
    let mut bots: Vec<Bot> = names.iter().map(|n| s.bot(n)).collect();
    for b in bots.iter_mut() {
        give_gold(&s, b, 30_000);
    }
    let start: Vec<u32> = bots.iter().map(gold).collect();
    // Uno + Tres vs Dos + Cuatro.
    propose(&mut bots[0], &["Dos", "Tres", "Cuatro"], BET);
    s.advance(50);
    bots.iter_mut().for_each(|b| {
        b.sync();
    });
    accept(&mut bots[1], "Uno");
    accept(&mut bots[1], "Uno");
    s.advance(50);
    bots.iter_mut().for_each(|b| {
        b.sync();
    });
    assert!(said(&bots[1], "Ya has aceptado este reto"));
    let pending = bots[0].game.local.p4.event.view.pending.clone().expect("still pending");
    assert_eq!(pending.team_a, ["Uno", "Tres"]);
    assert_eq!(pending.team_b, ["Dos", "Cuatro"]);
    assert_eq!(pending.missing, ["Tres", "Cuatro"]);
    accept(&mut bots[2], "Uno");
    accept(&mut bots[3], "uno");
    s.advance(50);
    bots.iter_mut().for_each(|b| {
        b.sync();
    });
    for (b, g) in bots.iter().zip(&start) {
        assert!(in_duel(b), "{} in the duel", b.name());
        assert!(ARENAS.contains(&b.map()));
        assert_eq!(gold(b), g - BET, "{} paid the bet once", b.name());
    }
    let d = bots[0].game.local.p4.event.view.duel.clone().unwrap();
    assert!(d.mine_a && d.round == 1 && d.countdown > 0);
}

#[test]
fn moving_gold_during_the_duel_and_conceding() {
    let s = TestServer::start();
    let mut a = s.bot("Valiente");
    let mut b = s.bot("Cobarde");
    give_gold(&s, &mut a, 20_000);
    give_gold(&s, &mut b, 20_000);
    let home = a.map();
    propose(&mut a, &["Cobarde"], BET);
    step(&s, &mut [&mut a, &mut b]);
    accept(&mut b, "Valiente");
    step(&s, &mut [&mut a, &mut b]);
    assert!(in_duel(&a) && in_duel(&b));
    let (ga, gb) = (gold(&a), gold(&b));
    assert_eq!((ga, gb), (10_000, 10_000));

    // The bet is held by the room: dropping gold now moves only what is in hand, and an
    // impossible amount is refused.
    b.send(&C2S::DropGold { amount: 5_000 });
    b.send(&C2S::DropGold { amount: 50_000 });
    step(&s, &mut [&mut a, &mut b]);
    assert_eq!(gold(&b), gb - 5_000);
    let gb = gold(&b);

    // Walking is held during the countdown (TiempoConteo = 15 s), and trying hard to walk
    // is not punished as a cheat (a kick would lose the bet).
    let pos = a.pos();
    for i in 0..30 {
        a.walk_raw(if i % 2 == 0 { ao_rules::Dir::Down } else { ao_rules::Dir::Right });
        // At a legit pace (faster would be a speedhack strike, not this system's business).
        s.advance(400);
    }
    step(&s, &mut [&mut a, &mut b]);
    assert!(!a.closed(), "not kicked for walking during the countdown");
    assert_eq!(a.pos(), pos, "held in place");

    // Conceding: the rival takes the pot minus 10 %; both back home.
    ev(&mut b, EventC2S::DuelQuit);
    ev(&mut b, EventC2S::DuelQuit);
    step(&s, &mut [&mut a, &mut b]);
    assert!(said(&b, "Has abandonado el reto"));
    assert_eq!(gold(&a), ga + 18_000);
    assert_eq!(gold(&b), gb, "the second concession does nothing");
    assert_eq!((a.map(), b.map()), (home, home));
    assert!(!in_duel(&a) && !in_duel(&b));
}

#[test]
fn disconnecting_halfway_neither_duplicates_nor_loses_gold() {
    let s = TestServer::start();
    let mut a = s.bot("Firme");
    let mut b = s.bot("Fugaz");
    give_gold(&s, &mut a, 10_000);
    give_gold(&s, &mut b, 10_000);
    // Disconnecting with a request pending: cancelled, nobody charged.
    propose(&mut a, &["Fugaz"], BET);
    step(&s, &mut [&mut a, &mut b]);
    drop(b);
    step(&s, &mut [&mut a]);
    assert!(said(&a, "Fugaz se ha desconectado"));
    assert!(a.game.local.p4.event.view.pending.is_none());
    assert_eq!(gold(&a), 10_000);

    // Disconnecting during the duel: the one who stays wins.
    let mut b = s.bot("Fugaz");
    give_gold(&s, &mut b, 10_000);
    propose(&mut a, &["Fugaz"], BET);
    step(&s, &mut [&mut a, &mut b]);
    accept(&mut b, "Firme");
    step(&s, &mut [&mut a, &mut b]);
    assert_eq!((gold(&a), gold(&b)), (0, 0));
    drop(b);
    step(&s, &mut [&mut a]);
    s.advance(1000);
    a.sync();
    assert_eq!(gold(&a), 18_000);
    assert!(!in_duel(&a));
}

#[test]
fn auction_house_with_bids_refunds_and_the_commission() {
    let s = TestServer::start();
    let item = auction_obj();
    let mut seller = s.bot("Vendedora");
    let mut x = s.bot("Postor");
    let mut y = s.bot("Rico");
    give_gold(&s, &mut seller, 1_000);
    give_gold(&s, &mut x, 5_000);
    give_gold(&s, &mut y, 5_000);
    seller.say(&format!("/dar {item} 3"));
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    let slot = seller.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == item)).unwrap() as u8 + 1;

    // Publishing: bad amounts and durations are refused.
    ev(&mut seller, EventC2S::AuctionPublish { slot, amount: 4, base_price: 1000, minutes: 5 });
    ev(&mut seller, EventC2S::AuctionPublish { slot, amount: 3, base_price: 0, minutes: 5 });
    ev(&mut seller, EventC2S::AuctionPublish { slot, amount: 3, base_price: 1000, minutes: 0 });
    ev(&mut seller, EventC2S::AuctionPublish { slot: 40, amount: 1, base_price: 1000, minutes: 5 });
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    assert!(seller.game.local.p4.event.lots.is_empty());
    ev(&mut seller, EventC2S::AuctionPublish { slot, amount: 3, base_price: 1000, minutes: 5 });
    ev(&mut seller, EventC2S::AuctionPublish { slot, amount: 3, base_price: 1000, minutes: 5 });
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    assert_eq!(gold(&seller), 500, "500 to publish, once");
    assert!(seller.game.local.inventory.iter().flatten().all(|i| i.obj != item), "held by the auction");
    let lots = x.game.local.p4.event.lots.clone();
    assert_eq!(lots.len(), 1);
    let lot = lots[0].id;
    assert_eq!((lots[0].min_bid, lots[0].amount), (1000, 3));

    // Below the base price, on your own lot, more than you have.
    ev(&mut x, EventC2S::AuctionBid { lot, amount: 999 });
    ev(&mut seller, EventC2S::AuctionBid { lot, amount: 1000 });
    ev(&mut x, EventC2S::AuctionBid { lot, amount: 5_001 });
    ev(&mut x, EventC2S::AuctionBid { lot: lot + 7, amount: 2000 });
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    assert!(said(&x, "La oferta mínima es de 1000"));
    assert!(said(&seller, "No podés auto ofertar"));
    assert!(said(&x, "No posees esa cantidad"));
    assert_eq!((gold(&x), gold(&seller)), (5_000, 500));

    // X bids, Y outbids by less than 100 (refused), then properly: X gets the bid back.
    ev(&mut x, EventC2S::AuctionBid { lot, amount: 1000 });
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    assert_eq!(gold(&x), 4_000);
    ev(&mut y, EventC2S::AuctionBid { lot, amount: 1099 });
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    assert_eq!(gold(&y), 5_000);
    ev(&mut y, EventC2S::AuctionBid { lot, amount: 1100 });
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    assert_eq!((gold(&x), gold(&y)), (5_000, 3_900));
    assert_eq!(y.game.local.p4.event.lots[0].bidder, "Rico");

    // Five minutes later: Y gets the items, the seller 1100 minus 5 %.
    s.advance_secs(301);
    for b in [&mut seller, &mut x, &mut y] {
        b.sync();
    }
    assert!(y.game.local.p4.event.lots.is_empty());
    assert_eq!(y.game.local.inventory.iter().flatten().filter(|i| i.obj == item).map(|i| i.amount).sum::<u32>(), 3);
    assert_eq!(gold(&seller), 500 + 1_045);
    assert_eq!(gold(&x), 5_000);
}

#[test]
fn an_outbid_offline_bidder_gets_the_gold_on_the_next_login_and_unsold_lots_go_back() {
    let s = TestServer::start();
    let item = auction_obj();
    let mut seller = s.bot("Duenno");
    let mut x = s.bot("Ausente");
    let mut y = s.bot("Presente");
    give_gold(&s, &mut seller, 500);
    give_gold(&s, &mut x, 2_000);
    give_gold(&s, &mut y, 2_000);
    seller.say(&format!("/dar {item} 1"));
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    let slot = seller.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == item)).unwrap() as u8 + 1;
    ev(&mut seller, EventC2S::AuctionPublish { slot, amount: 1, base_price: 1000, minutes: 2 });
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    let lot = x.game.local.p4.event.lots[0].id;
    ev(&mut x, EventC2S::AuctionBid { lot, amount: 1000 });
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    assert_eq!(gold(&x), 1_000);
    drop(x);
    step(&s, &mut [&mut seller, &mut y]);
    // Last minute: the bid extends the lot by 30 s.
    s.advance_secs(70);
    ev(&mut y, EventC2S::Refresh);
    step(&s, &mut [&mut seller, &mut y]);
    let before = y.game.local.p4.event.lots[0].secs_left;
    assert!(before < 60);
    ev(&mut y, EventC2S::AuctionBid { lot, amount: 1100 });
    step(&s, &mut [&mut seller, &mut y]);
    assert!(y.game.local.p4.event.lots[0].secs_left > before + 25, "extended");
    // Back in: the outbid bid is waiting in the mailbox.
    let mut x = s.bot("Ausente");
    s.advance(50);
    x.sync();
    assert_eq!(gold(&x), 1_000, "fresh dev character + the 1000 refunded");
    assert!(said(&x, "Te superaron en una subasta"));

    // A lot nobody bids on goes back to the seller.
    y.say(&format!("/dar {item} 2"));
    step(&s, &mut [&mut seller, &mut x, &mut y]);
    let yslot = y.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == item)).unwrap() as u8 + 1;
    ev(&mut y, EventC2S::AuctionPublish { slot: yslot, amount: 2, base_price: 50, minutes: 1 });
    s.advance_secs(120);
    for b in [&mut seller, &mut x, &mut y] {
        b.sync();
    }
    assert!(said(&y, "terminó sin ofertas"));
    assert_eq!(y.game.local.inventory.iter().flatten().filter(|i| i.obj == item).map(|i| i.amount).sum::<u32>(), 2 + 1, "2 back + 1 won");
}

#[test]
fn seller_and_winner_offline_when_the_lot_closes_get_it_by_mail() {
    let s = TestServer::start();
    let item = auction_obj();
    let mut seller = s.bot("Viajera");
    let mut buyer = s.bot("Dormido");
    give_gold(&s, &mut seller, 500);
    give_gold(&s, &mut buyer, 3_000);
    seller.say(&format!("/dar {item} 2"));
    step(&s, &mut [&mut seller, &mut buyer]);
    let slot = seller.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == item)).unwrap() as u8 + 1;
    ev(&mut seller, EventC2S::AuctionPublish { slot, amount: 2, base_price: 2_000, minutes: 1 });
    step(&s, &mut [&mut seller, &mut buyer]);
    let lot = buyer.game.local.p4.event.lots[0].id;
    ev(&mut buyer, EventC2S::AuctionBid { lot, amount: 2_000 });
    step(&s, &mut [&mut seller, &mut buyer]);
    assert_eq!(gold(&buyer), 1_000);
    // Both leave before the end.
    drop(seller);
    drop(buyer);
    let mut watcher = s.bot("Testigo");
    s.advance_secs(120);
    watcher.sync();
    assert!(watcher.game.local.p4.event.lots.is_empty());
    let mut seller = s.bot("Viajera");
    let mut buyer = s.bot("Dormido");
    step(&s, &mut [&mut seller, &mut buyer]);
    assert_eq!(gold(&seller), 1_900, "2000 minus 5 %, waiting in the mailbox");
    assert_eq!(buyer.game.local.inventory.iter().flatten().filter(|i| i.obj == item).map(|i| i.amount).sum::<u32>(), 2);
    // Delivered once: a second login brings nothing.
    drop(seller);
    let seller = s.bot("Viajera");
    assert_eq!(gold(&seller), 0);
}

/// Account characters (saved to the database): disconnecting halfway through a duel is
/// saved outside the ring, alive, with the bet lost.
#[test]
fn an_account_character_leaving_a_duel_is_saved_home_and_alive() {
    let s = TestServer::with(|st| {
        st.start = Some((1, 50, 50));
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = (1, 50, 50);
    });
    let character = |name: &str| {
        let mut c = s.connect();
        assert!(c.create_account(&name.to_lowercase(), "secreta1"));
        assert!(c.create_character(name, 1, 1, 1, 1));
        c.play(name);
        c.wait_until("en el mundo", |b| b.game.me().is_some() && b.game.stats.max_hp.get() > 0);
        c
    };
    let mut a = character("Guardian");
    let mut b = character("Desertor");
    give_gold(&s, &mut a, 10_000);
    give_gold(&s, &mut b, 10_000);
    let (gb, home) = (gold(&b), b.map());
    propose(&mut a, &["Desertor"], BET);
    step(&s, &mut [&mut a, &mut b]);
    accept(&mut b, "Guardian");
    step(&s, &mut [&mut a, &mut b]);
    assert!(ARENAS.contains(&b.map()));
    drop(b);
    step(&s, &mut [&mut a]);
    let mut b = s.connect();
    assert!(b.account_login("desertor", "secreta1"));
    b.play("Desertor");
    b.wait_until("en el mundo", |b| b.game.me().is_some() && b.game.stats.max_hp.get() > 0);
    b.sync();
    assert_eq!(b.map(), home, "saved out of the ring");
    assert!(b.hp() > 0, "saved alive");
    assert_eq!(gold(&b), gb - BET);
    assert_eq!(gold(&a), 18_000);
}

#[test]
fn tournament_sign_up_and_start() {
    let s = TestServer::start();
    let mut gm = s.bot("Organizador");
    let mut a = s.bot("Luchador");
    let mut b = s.bot("Pobre");
    give_gold(&s, &mut a, 3_000);
    let spec = TournamentSpec { name: "Torneo 1v1".into(), rules: "Sin pociones".into(), min_level: 1, max_level: 50, slots: 8, fee: 1_000, classes: vec![], map: 324, x: 50, y: 50 };
    ev(&mut a, EventC2S::TournamentJoin);
    step(&s, &mut [&mut gm, &mut a, &mut b]);
    assert!(said(&a, "No hay ningún torneo abierto"));
    ev(&mut gm, EventC2S::TournamentOpen(spec.clone()));
    step(&s, &mut [&mut gm, &mut a, &mut b]);
    assert!(said(&b, "Están abiertas las inscripciones para: Torneo 1v1"));
    ev(&mut a, EventC2S::TournamentJoin);
    ev(&mut a, EventC2S::TournamentJoin);
    ev(&mut b, EventC2S::TournamentJoin);
    step(&s, &mut [&mut gm, &mut a, &mut b]);
    assert_eq!(gold(&a), 2_000, "the fee once");
    assert!(said(&b, "Necesitas 1000 monedas"));
    // Leaving before the start refunds the fee; joining again charges it again.
    ev(&mut a, EventC2S::TournamentLeave);
    step(&s, &mut [&mut gm, &mut a, &mut b]);
    assert_eq!(gold(&a), 3_000);
    ev(&mut a, EventC2S::TournamentJoin);
    step(&s, &mut [&mut gm, &mut a, &mut b]);
    let t = a.game.local.p4.event.view.tournament.clone().unwrap();
    assert!(t.me_signed && t.signed == ["Luchador"]);
    // A player in a tournament cannot duel.
    give_gold(&s, &mut b, 20_000);
    propose(&mut b, &["Luchador"], BET);
    step(&s, &mut [&mut gm, &mut a, &mut b]);
    assert!(said(&b, "Luchador no puede jugar un reto"));
    ev(&mut gm, EventC2S::TournamentStart);
    step(&s, &mut [&mut gm, &mut a, &mut b]);
    assert_eq!(a.map(), 324);
    assert!(said(&b, "Los elegidos para participar son: Luchador"));
    ev(&mut gm, EventC2S::TournamentClose);
    step(&s, &mut [&mut gm, &mut a, &mut b]);
    assert!(a.game.local.p4.event.view.tournament.is_none());
    assert_eq!(gold(&a), 2_000, "the fee of a tournament that was played is kept");
}
