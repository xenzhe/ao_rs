//! P3 exit scenario, headless: two players go through the client's own actions (the same
//! code the windows call) from an empty account to trading. Register and create a
//! character with `AccountFlow`, enter the world, whisper with a `/w` command typed in the
//! console, propose a trade by right-clicking the other's tile, offer gold, confirm and
//! accept on both sides, and check the gold moved. The screenshots in docs/capturas show
//! the same windows drawn.
use ao_rules::character::MALE;
use ao_testkit::{Bot, TestServer};
use client_core::p3::account::{valid_heads, AccountFlow};
use client_core::Screen;

/// From a fresh connection to standing in the world, only through the account screens' flow.
fn newcomer(s: &TestServer, user: &str, name: &str) -> Bot {
    let mut b = s.connect();
    let mut flow = AccountFlow::default();
    let step = |b: &mut Bot, flow: &mut AccountFlow| {
        b.wait_until("respuesta de cuenta", |b| b.game.account_response().is_some() || b.game.entered_world() || b.closed());
        flow.update(&mut b.game);
    };
    flow.register(&mut b.game, user, "secreta1");
    step(&mut b, &mut flow);
    assert_eq!(b.game.screen, Screen::Create, "new account: creation screen");
    let head = valid_heads(1, MALE, |_| true)[0];
    flow.create_character(&mut b.game, name, 1, 3, MALE, head);
    step(&mut b, &mut flow);
    b.wait_until("personaje en la lista", |b| b.game.characters.iter().any(|c| c.name == name));
    flow.play(&mut b.game, name);
    step(&mut b, &mut flow);
    assert_eq!(b.game.screen, Screen::Game, "{:?}", flow.notice);
    b.wait_until("en el mapa", |b| b.game.me().is_some());
    b
}

#[test]
fn from_new_account_to_a_trade_through_the_client() {
    let s = TestServer::start();
    let mut ana = newcomer(&s, "ana_p3", "Anabel");
    let mut beto = newcomer(&s, "beto_p3", "Bertoldo");
    ana.sync();
    beto.sync();

    // ---- console: a whisper typed as a command reaches the other one.
    assert!(ana.game.submit_chat("/w Bertoldo hola, ¿comerciamos?"));
    beto.wait_until("susurro recibido", |b| b.game.local.social.channel.history.iter().any(|l| l.text.contains("comerciamos")));
    assert_eq!(beto.game.local.social.channel.last_whisper_from.as_deref(), Some("Anabel"));
    // `/r` answers whoever whispered last.
    assert!(beto.game.submit_chat("/r dale"));
    ana.wait_until("respuesta", |b| b.game.local.social.channel.history.iter().any(|l| l.text == "dale"));

    // ---- trade: Beto right-clicks Ana's tile (both spawn next to each other in the city).
    let (ax, ay) = ana.pos();
    beto.sync();
    assert!(beto.game.trade_with_tile(ax, ay), "right click on a nearby player proposes a trade");
    ana.wait_until("invitacion", |b| b.game.local.social.trade.invited_by.is_some());
    assert!(ana.game.trade_accept_invite());
    beto.wait_until("comercio abierto", |b| b.game.local.social.trade.player.is_some());
    ana.wait_until("comercio abierto", |b| b.game.local.social.trade.player.is_some());

    // A new character may have no gold: the dev command gives some to trade.
    if ana.game.local.gold < 100 {
        assert!(ana.game.submit_chat("/dar 12 500"));
        ana.wait_until("oro", |b| b.game.local.gold >= 100);
    }
    let (ag, bg) = (ana.game.local.gold, beto.game.local.gold);
    assert!(ana.game.trade_offer_gold(100));
    beto.wait_until("ve la oferta", |b| b.game.local.social.trade.player.as_ref().is_some_and(|t| t.theirs.gold == 100));
    ana.wait_until("revision al dia", |b| b.game.local.social.trade.player.as_ref().is_some_and(|t| t.mine.gold == 100));
    assert!(ana.game.trade_confirm());
    assert!(beto.game.trade_confirm());
    ana.wait_until("ambos confirmaron", |b| b.game.local.social.trade.player.as_ref().is_some_and(|t| t.i_confirmed && t.they_confirmed));
    beto.wait_until("ambos confirmaron", |b| b.game.local.social.trade.player.as_ref().is_some_and(|t| t.i_confirmed && t.they_confirmed));
    assert!(ana.game.trade_accept());
    assert!(beto.game.trade_accept());
    beto.wait_until("oro recibido", |b| b.game.local.gold == bg + 100);
    ana.wait_until("oro entregado", |b| b.game.local.gold == ag - 100);
    assert!(ana.kicked().is_none() && beto.kicked().is_none());
}
