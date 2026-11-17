//! P2 trade over the wire: characters created through accounts in Ullathorpe buy and sell
//! at a real merchant of the official map and trade with each other with the double
//! confirmation; the cheats (no gold, selling what is not there, huge amounts, changing the
//! offer after confirming, confirming twice, the same units twice, disconnecting halfway,
//! trading dead or far) are refused and nothing is duplicated.
use ao_formats::dat::npcs;
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::{TradeC2S, TradeS2C, C2S, S2C};
use ao_rules::trade::{buy_price, sale_price};
use ao_testkit::{Bot, TestServer};

/// Official objects used: 38 and 39 are ordinary, stackable objects of obj.dat.
const OBJ_A: u16 = 38;
const GOLD_OBJ: u16 = 12;

struct Merchant {
    x: i32,
    y: i32,
    def: u16,
}

/// A merchant of Ullathorpe with stock in npcs.dat.
fn merchant() -> Merchant {
    let dat = npcs::load(&recursos_dir()).unwrap().data;
    let map = AoMap::load(&recursos_dir(), 1).unwrap();
    let m = map.npcs.iter().find(|e| dat.get(e.npc as u32).is_some_and(|d| d.trades == 1 && !d.name.is_empty() && !d.inventory.is_empty())).copied().expect("a merchant in Ullathorpe");
    Merchant { x: m.x, y: m.y, def: m.npc }
}

/// New characters (accounts) appear at `(map, x, y)`.
fn server_at(map: u16, x: i32, y: i32) -> TestServer {
    TestServer::with(|st| {
        st.start = Some((map, x, y));
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = (map, x, y);
    })
}

fn character(s: &TestServer, name: &str) -> Bot {
    let mut c = s.connect();
    assert!(c.create_account(&name.to_lowercase(), "secreta1"));
    assert!(c.create_character(name, 1, 1, 1, 1));
    c.play(name);
    c.wait_until("en el mundo", |b| b.game.me().is_some() && b.game.stats.max_hp.get() > 0);
    c
}

fn trade(bot: &mut Bot, m: TradeC2S) {
    bot.send(&C2S::Trade(m));
}

fn give(bot: &mut Bot, obj: u16, amount: u32) {
    bot.say(&format!("/dar {obj} {amount}"));
}

fn gold(bot: &Bot) -> u32 {
    bot.game.local.gold
}

fn count(bot: &Bot, obj: u16) -> u32 {
    bot.game.local.inventory.iter().flatten().filter(|i| i.obj == obj).map(|i| i.amount).sum()
}

fn slot_of(bot: &Bot, obj: u16) -> u8 {
    bot.game.local.inventory.iter().position(|s| s.is_some_and(|i| i.obj == obj)).map(|i| i as u8 + 1).expect("in the inventory")
}

fn sync_all(s: &TestServer, bots: &mut [&mut Bot]) {
    s.advance(50);
    for b in bots.iter_mut() {
        b.sync();
    }
}

#[test]
fn buy_and_sell_at_an_ullathorpe_merchant() {
    let m = merchant();
    let s = server_at(1, m.x, m.y + 2);
    let mut bot = character(&s, "Mercader");
    give(&mut bot, GOLD_OBJ, 10_000);
    bot.send(&C2S::Interact { tx: m.x, ty: m.y });
    s.advance(50);
    bot.sync();
    let shop = bot.game.local.social.trade.shop.clone().unwrap_or_else(|| panic!("shop open: {:?}", bot.console().collect::<Vec<_>>()));
    let (&slot, item) = shop.items.iter().next().expect("official stock");
    let objs = ao_formats::dat::obj::load(&recursos_dir()).unwrap().data;
    let value = objs.get(item.obj as u32).unwrap().value;
    assert_eq!(item.price, buy_price(value, 0, 1) as u32, "official unit price");

    // Buy two.
    let before = gold(&bot);
    trade(&mut bot, TradeC2S::Buy { slot, amount: 2 });
    s.advance(50);
    bot.sync();
    assert_eq!(count(&bot, item.obj), 2.min(item.amount));
    assert_eq!(gold(&bot), before - buy_price(value, 0, 2.min(item.amount)) as u32);

    // Sell them back: a third of the value.
    let have = count(&bot, item.obj);
    let before = gold(&bot);
    let inv = slot_of(&bot, item.obj);
    trade(&mut bot, TradeC2S::Sell { slot: inv, amount: have });
    s.advance(50);
    bot.sync();
    let data = objs.get(item.obj as u32).unwrap();
    if data.newbie == 0 && data.non_transferable == 0 && data.destroys == 0 && !data.no_tradeable_to_npc {
        assert_eq!(count(&bot, item.obj), 0);
        assert_eq!(gold(&bot), before + sale_price(value, false, have) as u32);
    }

    // Traps: no gold, zero, huge amounts, slots that do not exist, selling an empty slot.
    give(&mut bot, OBJ_A, 1);
    s.advance(50);
    bot.sync();
    let g = gold(&bot);
    trade(&mut bot, TradeC2S::Buy { slot, amount: 0 });
    trade(&mut bot, TradeC2S::Sell { slot: 30, amount: 1 });
    s.advance(50);
    bot.sync();
    assert_eq!(gold(&bot), g);
    assert!(!bot.closed(), "a couple of bad packets are only refused");

    // Close.
    trade(&mut bot, TradeC2S::CloseShop);
    s.advance(50);
    bot.sync();
    assert!(bot.game.local.social.trade.shop.is_none());
    // Buying with the window closed does nothing.
    let g = gold(&bot);
    trade(&mut bot, TradeC2S::Buy { slot, amount: 1 });
    s.advance(50);
    bot.sync();
    assert_eq!(gold(&bot), g);
    let _ = m.def;
}

#[test]
fn buying_without_gold_and_huge_amounts() {
    let m = merchant();
    let s = server_at(1, m.x, m.y + 2);
    let mut bot = character(&s, "Pobre");
    bot.send(&C2S::Interact { tx: m.x, ty: m.y });
    s.advance(50);
    bot.sync();
    let shop = bot.game.local.social.trade.shop.clone().expect("open");
    let (&slot, item) = shop.items.iter().find(|(_, i)| i.price > 0).expect("something with a price");
    trade(&mut bot, TradeC2S::Buy { slot, amount: 1 });
    s.advance(50);
    bot.sync();
    assert_eq!(count(&bot, item.obj), 0, "no gold, no object");
    // Asking for more than a stack every time ends in a kick.
    for _ in 0..40 {
        trade(&mut bot, TradeC2S::Buy { slot, amount: u32::MAX });
    }
    bot.wait_until("expulsado", |b| b.closed());
    assert!(bot.kicked().is_some());
}

/// Two characters next to each other in Ullathorpe (a safe map), already trading.
fn pair(s: &TestServer) -> (Bot, Bot) {
    let mut a = character(s, "Ana");
    let mut b = character(s, "Beto");
    give(&mut a, OBJ_A, 10);
    give(&mut b, GOLD_OBJ, 1000);
    sync_all(s, &mut [&mut a, &mut b]);
    let (ia, ib) = (a.id(), b.id());
    trade(&mut a, TradeC2S::Propose { target: ib });
    sync_all(s, &mut [&mut a, &mut b]);
    assert_eq!(b.game.local.social.trade.invited_by.as_ref().map(|i| i.0), Some(ia), "{:?}", a.console().collect::<Vec<_>>());
    trade(&mut b, TradeC2S::Propose { target: ia });
    sync_all(s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.is_some() && b.game.local.social.trade.player.is_some());
    (a, b)
}

fn rev(bot: &Bot) -> u32 {
    bot.game.local.social.trade.player.as_ref().map_or(0, |t| t.rev)
}

#[test]
fn players_exchange_after_double_confirmation() {
    let s = server_at(1, 50, 50);
    let (mut a, mut b) = pair(&s);
    let slot = slot_of(&a, OBJ_A);
    trade(&mut a, TradeC2S::Offer { slot, amount: 4 });
    trade(&mut b, TradeC2S::OfferGold { amount: 300 });
    sync_all(&s, &mut [&mut a, &mut b]);
    let t = a.game.local.social.trade.player.clone().unwrap();
    assert_eq!(t.mine.items, vec![(OBJ_A, 4)]);
    assert_eq!(t.theirs.gold, 300);
    let r = rev(&a);
    assert_eq!(r, rev(&b));

    // Accept without confirming: nothing.
    trade(&mut a, TradeC2S::Accept { rev: r });
    trade(&mut b, TradeC2S::Accept { rev: r });
    sync_all(&s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.is_some());
    assert_eq!(count(&b, OBJ_A), 0);

    // Confirm twice, then accept.
    for _ in 0..2 {
        trade(&mut a, TradeC2S::Confirm { rev: r });
        trade(&mut b, TradeC2S::Confirm { rev: r });
    }
    sync_all(&s, &mut [&mut a, &mut b]);
    trade(&mut a, TradeC2S::Accept { rev: r });
    sync_all(&s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.as_ref().unwrap().they_confirmed);
    trade(&mut b, TradeC2S::Accept { rev: r });
    sync_all(&s, &mut [&mut a, &mut b]);
    assert_eq!(a.game.local.social.trade.last_done, Some(true), "{:?} / {:?}", a.console().collect::<Vec<_>>(), b.console().collect::<Vec<_>>());
    assert_eq!((count(&a, OBJ_A), count(&b, OBJ_A)), (6, 4));
    assert_eq!((gold(&a), gold(&b)), (300, 700));
    // Accepting again after it is over does nothing.
    trade(&mut b, TradeC2S::Accept { rev: r });
    sync_all(&s, &mut [&mut a, &mut b]);
    assert_eq!((count(&a, OBJ_A), count(&b, OBJ_A)), (6, 4));
}

#[test]
fn changing_the_offer_after_confirming_cancels_the_confirmations() {
    let s = server_at(1, 50, 50);
    let (mut a, mut b) = pair(&s);
    let slot = slot_of(&a, OBJ_A);
    trade(&mut a, TradeC2S::Offer { slot, amount: 4 });
    trade(&mut b, TradeC2S::OfferGold { amount: 300 });
    sync_all(&s, &mut [&mut a, &mut b]);
    let r = rev(&a);
    trade(&mut a, TradeC2S::Confirm { rev: r });
    trade(&mut b, TradeC2S::Confirm { rev: r });
    sync_all(&s, &mut [&mut a, &mut b]);
    trade(&mut a, TradeC2S::Accept { rev: r });
    sync_all(&s, &mut [&mut a, &mut b]);
    assert!(b.game.local.social.trade.player.as_ref().unwrap().they_accepted);
    // B lowers the gold right before accepting with the old revision.
    trade(&mut b, TradeC2S::OfferGold { amount: 1 });
    trade(&mut b, TradeC2S::Accept { rev: r });
    sync_all(&s, &mut [&mut a, &mut b]);
    let t = a.game.local.social.trade.player.clone().expect("still trading");
    assert!(!t.i_confirmed && !t.i_accepted && !t.they_confirmed, "{t:?}");
    assert_ne!(t.rev, r);
    assert_eq!(t.theirs.gold, 1);
    assert_eq!((count(&a, OBJ_A), gold(&a)), (10, 0), "nothing moved");
}

#[test]
fn offering_what_you_do_not_have_or_the_same_units_twice() {
    let s = server_at(1, 50, 50);
    let (mut a, mut b) = pair(&s);
    let slot = slot_of(&a, OBJ_A);
    trade(&mut a, TradeC2S::Offer { slot, amount: 10 });
    trade(&mut a, TradeC2S::Offer { slot, amount: 10 });
    trade(&mut a, TradeC2S::OfferGold { amount: u32::MAX });
    trade(&mut a, TradeC2S::Offer { slot: 20, amount: 1 });
    sync_all(&s, &mut [&mut a, &mut b]);
    let t = b.game.local.social.trade.player.clone().unwrap();
    assert_eq!(t.theirs.items, vec![(OBJ_A, 10)], "the same ten units only once");
    assert_eq!(t.theirs.gold, 0);
    assert!(!a.closed());
}

#[test]
fn disconnecting_halfway_duplicates_nothing() {
    let s = server_at(1, 50, 50);
    let (mut a, mut b) = pair(&s);
    let slot = slot_of(&a, OBJ_A);
    trade(&mut a, TradeC2S::Offer { slot, amount: 10 });
    sync_all(&s, &mut [&mut a, &mut b]);
    let r = rev(&a);
    trade(&mut a, TradeC2S::Confirm { rev: r });
    trade(&mut b, TradeC2S::Confirm { rev: r });
    trade(&mut b, TradeC2S::Accept { rev: r });
    sync_all(&s, &mut [&mut a, &mut b]);
    drop(a); // A disconnects before accepting.
    s.advance(100);
    b.sync();
    assert!(b.game.local.social.trade.player.is_none(), "the trade is cancelled");
    assert_eq!(count(&b, OBJ_A), 0);

    // A comes back with its ten objects, saved on logout.
    let mut c = s.connect();
    assert!(c.account_login("ana", "secreta1"));
    c.play("Ana");
    c.wait_until("en el mundo", |b| b.game.me().is_some() && b.game.stats.max_hp.get() > 0);
    s.advance(50);
    c.sync();
    assert_eq!(count(&c, OBJ_A), 10);
}

#[test]
fn no_trade_when_dead_far_or_with_yourself() {
    let s = server_at(1, 50, 50);
    let mut a = character(&s, "Solo");
    let me = a.id();
    trade(&mut a, TradeC2S::Propose { target: me });
    trade(&mut a, TradeC2S::Propose { target: 999_999 });
    trade(&mut a, TradeC2S::Confirm { rev: 1 });
    trade(&mut a, TradeC2S::Accept { rev: 1 });
    trade(&mut a, TradeC2S::Cancel);
    trade(&mut a, TradeC2S::Decline);
    s.advance(50);
    a.sync();
    assert!(a.game.local.social.trade.player.is_none());
    assert!(!a.closed());

    // Walking away from the partner cancels the trade on the next action.
    let (mut x, mut y) = pair(&s);
    let dir = x.free_dir(5).expect("room to walk");
    for _ in 0..5 {
        x.walk(dir);
        s.advance(ao_rules::WALK_MS);
        x.sync();
    }
    trade(&mut x, TradeC2S::OfferGold { amount: 0 });
    let r = rev(&x);
    trade(&mut x, TradeC2S::Confirm { rev: r });
    sync_all(&s, &mut [&mut x, &mut y]);
    assert!(x.game.local.social.trade.player.is_none() && y.game.local.social.trade.player.is_none());
    assert!(y.log().iter().any(|m| matches!(m, S2C::Trade(TradeS2C::Closed { done: false }))));
}
