//! P4 GM (Opus C): ranks from the database, commands with their permissions, jail that
//! survives reconnecting, bans that keep people out, silence on every channel, and the log.
use ao_protocol::p4::gm::GmRank;
use ao_protocol::social::channel::ChannelKind;
use ao_protocol::{ChannelC2S, ChannelS2C, GmC2S, GmS2C, C2S, PROTOCOL_VERSION, S2C};
use ao_testkit::{Bot, TestServer};
use client_core::Screen;

const CITY: (u16, i32, i32) = (1, 50, 50);
const JAIL_MAP: u16 = 66;

/// Every test of this binary seeds the same GMs (env var read when each world starts).
fn server() -> TestServer {
    std::env::set_var("AO_GM_SEED", "Admin:admin,Dios:dios,Semi:semidios,Conse:consejero");
    TestServer::with(|st| {
        st.npcs = false;
        st.start = Some(CITY);
    })
}

fn said(b: &Bot, needle: &str) -> bool {
    b.console().any(|l| l.contains(needle))
}

fn gm_msgs(b: &Bot) -> usize {
    b.count(|m| matches!(m, S2C::Gm(_)))
}

fn cmd(s: &TestServer, gm: &mut Bot, line: &str) {
    gm.say(line);
    s.settle();
    gm.sync();
}

/// Disconnects  and waits until witness hears it left (the server logged it out).
fn leave(b: Bot, witness: &mut Bot) {
    let line = format!("{} se desconecto.", b.name());
    drop(b);
    witness.wait_until("desconexion", |w| said(w, &line));
}

fn rank_of(b: &Bot) -> Option<GmRank> {
    b.log().iter().rev().find_map(|m| if let S2C::Gm(GmS2C::Rank { rank }) = m { Some(*rank) } else { None })
}

#[test]
fn ranks_come_from_the_database_and_only_gms_hear_about_them() {
    let s = server();
    let mut admin = s.bot("Admin");
    let mut pepe = s.bot("Pepe");
    admin.sync();
    pepe.sync();
    assert_eq!(rank_of(&admin), Some(GmRank::Admin));
    assert_eq!(gm_msgs(&pepe), 0, "a normal player is not told anything about GMs");

    // The panel answers GMs only.
    pepe.send(&C2S::Gm(GmC2S::Refresh));
    pepe.send(&C2S::Gm(GmC2S::Command { line: "/rmsg hackeado".into() }));
    admin.send(&C2S::Gm(GmC2S::Refresh));
    s.settle();
    pepe.sync();
    admin.sync();
    assert_eq!(gm_msgs(&pepe), 0);
    assert!(!said(&admin, "hackeado"));
    let online = admin.log().iter().rev().find_map(|m| if let S2C::Gm(GmS2C::Online { players }) = m { Some(players.clone()) } else { None }).expect("online list");
    assert_eq!(online.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Admin", "Pepe"]);
    assert!(admin.log().iter().any(|m| matches!(m, S2C::Gm(GmS2C::Log { .. }))));

    // An admin promotes; the promoted player learns his rank at once and keeps it.
    cmd(&s, &mut admin, "/rol Pepe@consejero");
    pepe.wait_until("rango nuevo", |b| rank_of(b) == Some(GmRank::Consejero));
    cmd(&s, &mut pepe, "/rmsg Hola a todos");
    admin.wait_until("anuncio", |b| said(b, "Servidor » Hola a todos"));
    leave(pepe, &mut admin);
    let pepe = s.bot("Pepe");
    assert_eq!(rank_of(&pepe), Some(GmRank::Consejero), "rank persists");
}

#[test]
fn a_normal_player_trying_gm_commands_changes_nothing_and_learns_nothing() {
    let s = server();
    let mut pepe = s.bot("Pepe");
    let mut beto = s.bot("Beto");
    let start = (beto.map(), beto.pos());
    for line in ["/carcel Beto@flood@10", "/ban Beto@cheat", "/sum Beto", "/echar Beto", "/rol Pepe@admin", "/ci 12 1000", "/silenciar Beto@60", "/apagar 1"] {
        pepe.say(line);
        s.advance(1000);
    }
    s.settle();
    pepe.sync();
    beto.sync();
    assert!(!beto.closed() && beto.kicked().is_none());
    assert_eq!((beto.map(), beto.pos()), start, "nobody moved Beto");
    // Same answer as any other unknown `/` line: no usage, no "not allowed".
    for hint in ["Uso:", "deshabilitado", "rango", "Servidor »", "Apagado"] {
        assert!(!said(&pepe, hint), "hint '{hint}' leaked: {:?}", pepe.console().collect::<Vec<_>>());
    }
    assert_eq!(gm_msgs(&pepe), 0);
    // Beto can still talk (was not silenced).
    beto.say("sigo libre");
    pepe.wait_until("Beto habla", |b| said(b, "Beto: sigo libre"));
    // And Pepe did not become admin.
    leave(pepe, &mut beto);
    let pepe = s.bot("Pepe");
    assert_eq!(gm_msgs(&pepe), 0);
}

#[test]
fn a_consejero_cannot_do_what_needs_a_god() {
    let s = server();
    let mut conse = s.bot("Conse");
    let mut semi = s.bot("Semi");
    let mut beto = s.bot("Beto");
    cmd(&s, &mut conse, "/carcel Beto@flood@10");
    cmd(&s, &mut conse, "/ban Beto@x");
    cmd(&s, &mut conse, "/sum Beto");
    cmd(&s, &mut conse, "/telep Beto 1 30 30");
    beto.sync();
    assert_eq!(beto.map(), CITY.0);
    assert!(beto.kicked().is_none());
    assert_eq!(conse.console().filter(|l| l.contains("deshabilitado para tu cargo")).count(), 4);

    // What a Consejero may do, but only on lower ranks.
    cmd(&s, &mut conse, "/silenciar Semi@5");
    assert!(said(&conse, "mayor o igual rango"));
    semi.sync();
    semi.say("hablo");
    conse.wait_until("Semi sigue hablando", |b| said(b, "Semi: hablo"));
    cmd(&s, &mut conse, "/telep YO 1 40 40");
    conse.wait_until("se movió a sí mismo", |b| b.pos() != (CITY.1, CITY.2));
    // A SemiDios cannot ban a god either, and cannot unban (Dios).
    cmd(&s, &mut semi, "/ban Conse@x");
    cmd(&s, &mut semi, "/ban Admin@x");
    assert!(said(&semi, "mayor o igual rango"));
    cmd(&s, &mut semi, "/unban Beto");
    assert!(said(&semi, "deshabilitado para tu cargo"));
}

#[test]
fn jail_counts_down_and_survives_reconnecting() {
    let s = server();
    let mut semi = s.bot("Semi");
    let mut beto = s.bot("Beto");
    cmd(&s, &mut semi, "/carcel Beto@insultos@2");
    beto.wait_until("en la carcel", |b| b.map() == JAIL_MAP);
    assert!(said(&beto, "Semi te ha encarcelado"));

    // A GM cannot be jailed.
    cmd(&s, &mut semi, "/carcel Semi@x@5");
    assert_eq!(semi.map(), CITY.0);

    // One minute served, then he leaves and comes back: still in jail, one minute left.
    s.advance(61_000);
    leave(beto, &mut semi);
    s.advance(5 * 60_000); // offline time does not count
    let mut beto = s.bot("Beto");
    assert_eq!(beto.map(), JAIL_MAP, "still jailed after reconnecting");
    assert!(said(&beto, "Te quedan 1 minutos"), "{:?}", beto.console().collect::<Vec<_>>());

    // Leaving the jail by any means puts him back.
    beto.say("/ir 1 50 50"); // dev command; a jailed player is sent back on the next tick
    s.advance(1000);
    beto.sync();
    assert_eq!(beto.map(), JAIL_MAP);

    s.advance(61_000);
    beto.wait_until("liberado", |b| said(b, "Has sido liberado"));
    assert_eq!(beto.map(), JAIL_MAP, "[Libertad] is on the jail map, outside the cell");

    // History and log.
    cmd(&s, &mut semi, "/penas Beto");
    assert!(said(&semi, "carcel por Semi 2 min: insultos"));
    cmd(&s, &mut semi, "/log Semi");
    assert!(said(&semi, "carcel Beto"));
}

#[test]
fn banned_characters_accounts_and_addresses_cannot_enter() {
    let s = server();
    let mut semi = s.bot("Semi");
    let mut dios = s.bot("Dios");

    // An account character, banned while playing.
    let mut malo = s.connect();
    assert!(malo.create_account("cuentamala", "secreta1"));
    assert!(malo.create_character("Malo", 1, 3, 1, 1));
    malo.play("Malo");
    cmd(&s, &mut semi, "/ban Malo@uso de cheats");
    malo.wait_until("baneado", |b| b.kicked().is_some_and(|r| r.contains("baneado")));

    // The account still logs in, but the character is refused on entering.
    let mut again = s.connect();
    assert!(again.account_login("cuentamala", "secreta1"));
    again.send(&C2S::PlayCharacter { name: "Malo".into() });
    again.game.screen = Screen::Game;
    s.advance(100); // the refusal is applied on the next tick
    again.wait_until("rechazado", |b| b.kicked().is_some_and(|r| r.contains("uso de cheats")));

    // The whole account (offline): the login itself is refused.
    cmd(&s, &mut dios, "/bancuenta Malo@reincidente");
    let mut third = s.connect();
    assert!(!third.account_login("cuentamala", "secreta1"));
    cmd(&s, &mut dios, "/unbancuenta Malo");
    cmd(&s, &mut dios, "/unban Malo");
    let mut fourth = s.connect();
    assert!(fourth.account_login("cuentamala", "secreta1"));
    fourth.play("Malo");
    assert!(fourth.kicked().is_none());

    // By address: everybody from there is out, except GMs.
    let mut beto = s.bot("Beto");
    cmd(&s, &mut dios, "/banip Beto@multicuenta");
    beto.wait_until("ip baneada", |b| b.kicked().is_some());
    s.settle();
    dios.sync();
    assert!(!dios.closed() && dios.kicked().is_none(), "GMs are never IP-banned");
    let mut nuevo = s.connect();
    nuevo.send(&C2S::Login { name: "Nuevo".into(), version: PROTOCOL_VERSION });
    nuevo.game.screen = Screen::Game;
    s.advance(100); // the refusal is applied on the next tick
    nuevo.wait_until("ip rechazada", |b| b.kicked().is_some_and(|r| r.contains("multicuenta")));
    nuevo.wait_until("cerrado", |b| b.closed());
    dios.wait_until("Nuevo fuera", |b| said(b, "Nuevo se desconecto."));
    cmd(&s, &mut dios, "/unbanip 127.0.0.1");
    let nuevo = s.bot("Nuevo");
    assert!(nuevo.kicked().is_none());

    cmd(&s, &mut semi, "/penas Malo");
    assert!(said(&semi, "ban por Semi"));
    assert!(said(&semi, "bancuenta por Dios"));
}

#[test]
fn a_silenced_player_is_heard_on_no_channel_until_it_ends() {
    let s = server();
    let mut conse = s.bot("Conse");
    let mut beto = s.bot("Beto");
    let mut ana = s.bot("Ana");
    cmd(&s, &mut conse, "/silenciar Beto@1");
    beto.wait_until("silenciado", |b| said(b, "Has sido silenciado"));

    beto.say("hola mapa");
    for msg in [
        ChannelC2S::Whisper { to: "Ana".into(), text: "hola susurro".into() },
        ChannelC2S::Yell { text: "hola grito".into() },
        ChannelC2S::Party { text: "hola grupo".into() },
        ChannelC2S::Guild { text: "hola clan".into() },
        ChannelC2S::Global { text: "hola global".into() },
    ] {
        beto.send(&C2S::Channel(msg));
    }
    s.settle();
    beto.sync();
    ana.sync();
    assert!(!ana.console().any(|l| l.contains("hola")), "{:?}", ana.console().collect::<Vec<_>>());
    assert_eq!(ana.count(|m| matches!(m, S2C::Channel(ChannelS2C::Message { .. }))), 0);
    assert_eq!(beto.console().filter(|l| l.contains("Estás silenciado")).count(), 6);

    // Reconnecting does not lift it.
    leave(beto, &mut conse);
    let mut beto = s.bot("Beto");
    assert!(said(&beto, "Estás silenciado"));
    beto.say("todavia no");
    s.settle();
    ana.sync();
    assert!(!said(&ana, "todavia no"));

    s.advance(61_000);
    beto.wait_until("desilenciado", |b| said(b, "Has sido desilenciado"));
    beto.send(&C2S::Channel(ChannelC2S::Whisper { to: "Ana".into(), text: "ya puedo".into() }));
    ana.wait_until("susurro", |b| b.log().iter().any(|m| matches!(m, S2C::Channel(ChannelS2C::Message { channel: ChannelKind::Whisper, text, .. }) if text == "ya puedo")));
}

#[test]
fn moving_people_items_npcs_and_the_shutdown() {
    let s = server();
    let mut admin = s.bot("Admin");
    let mut beto = s.bot("Beto");
    cmd(&s, &mut admin, "/telep YO 1 30 30");
    admin.wait_until("teleport", |b| b.pos() != (CITY.1, CITY.2));
    cmd(&s, &mut admin, "/sum Beto");
    beto.wait_until("traido", |b| said(b, "Admin te ha trasportado"));
    cmd(&s, &mut admin, "/telep YO 1 70 70");
    cmd(&s, &mut admin, "/ira Beto");
    admin.wait_until("fue a Beto", |b| (b.pos().0 - 30).abs() <= 3 && (b.pos().1 - 30).abs() <= 3);

    cmd(&s, &mut admin, "/info Beto");
    assert!(said(&admin, "Personaje: Beto (Usuario)"));
    cmd(&s, &mut admin, "/dar Beto@12@500@premio");
    beto.wait_until("oro", |b| b.game.local.gold >= 500);
    cmd(&s, &mut admin, "/mod Beto ORO 7");
    beto.wait_until("oro fijado", |b| b.game.local.gold == 7);

    cmd(&s, &mut admin, "/ejecutar Beto");
    beto.wait_until("muerto", |b| b.hp() == 0);
    cmd(&s, &mut admin, "/revivir Beto");
    beto.wait_until("vivo", |b| b.hp() > 0);

    // An NPC created without respawn, then killed for good.
    let before = admin.creatures().count();
    cmd(&s, &mut admin, "/acc 500");
    admin.wait_until("npc creado", |b| b.creatures().count() == before + 1);
    cmd(&s, &mut admin, "/mata");
    admin.wait_until("npc borrado", |b| b.creatures().count() == before);

    cmd(&s, &mut admin, "/invisible");
    assert!(said(&admin, "Ahora eres invisible"));

    cmd(&s, &mut admin, "/echar Beto");
    beto.wait_until("echado", |b| b.kicked().is_some_and(|r| r.contains("Admin te ha echado")));

    // Shutdown with notice: everybody out at the end, then nobody gets in.
    cmd(&s, &mut admin, "/apagar 1");
    admin.wait_until("aviso", |b| said(b, "Apagado en 1 minutos"));
    s.advance(61_000);
    admin.wait_until("apagado", |b| b.kicked().is_some_and(|r| r.contains("apagado")));
    let mut late = s.connect();
    late.send(&C2S::Login { name: "Tarde".into(), version: PROTOCOL_VERSION });
    late.game.screen = Screen::Game;
    s.advance(100); // the refusal is applied on the next tick
    late.wait_until("rechazado", |b| b.kicked().is_some());
}
