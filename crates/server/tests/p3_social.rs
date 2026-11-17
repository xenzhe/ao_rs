//! P3 social actions of the client (`client_core::p3::social`) against the real server:
//! the typed `/` commands, the merged console, party actions, the guild actions of the
//! guild window (found, apply, accept, news, kick, relations) and taking a quest from the
//! NPC's offer. Bots on a virtual clock.
use ao_protocol::social::channel::ChannelKind;
use ao_rules::guild::{FOUNDATION_GEMS, SKILL_LEADERSHIP};
use ao_testkit::{Bot, TestServer};
use server::db::{Db, NewCharacter};
use std::path::PathBuf;

const PASS: &str = "secreta1";
const CITY: (u16, i32, i32) = (1, 50, 50);

/// Map 2 (not a safe zone), no creatures.
fn field() -> TestServer {
    TestServer::with(|st| {
        st.npcs = false;
        st.start = Some((2, 50, 50));
        st.data.balance.creatures_per_map = 0;
    })
}

fn settle(s: &TestServer, bots: &mut [&mut Bot]) {
    s.settle();
    for b in bots.iter_mut() {
        b.sync();
    }
}

fn console_has(b: &Bot, needle: &str) -> bool {
    b.console().any(|l| l.contains(needle))
}

#[test]
fn typed_commands_reach_their_channels_and_the_feed_orders_them() {
    let s = field();
    let (mut ana, mut bea) = (s.bot("Ana"), s.bot("Bea"));
    // Map chat, whisper, reply, yell and an unknown `/` line (said as chat, not a channel).
    assert!(ana.game.submit_chat("hola mapa"));
    assert!(ana.game.submit_chat("/s Bea nos vemos luego"));
    bea.wait_until("susurro", |b| b.game.local.social.channel.last_whisper_from.as_deref() == Some("Ana"));
    assert!(bea.game.submit_chat("/r vale, hasta luego"));
    assert!(bea.game.submit_chat("/y ¡a todos!"));
    ana.wait_until("grito", |b| b.game.local.social.channel.lines(ChannelKind::Yell).any(|l| l.author == "Bea"));
    ana.wait_until("respuesta", |b| b.game.local.social.channel.lines(ChannelKind::Whisper).any(|l| l.author == "Bea"));
    bea.wait_until("chat del mapa", |b| b.console().any(|l| l.contains("Ana: hola mapa")));
    assert!(console_has(&bea, "Ana: hola mapa"));
    assert!(bea.game.local.social.channel.lines(ChannelKind::Whisper).any(|l| l.author == "Ana" && l.text == "nos vemos luego"));
    assert!(ana.game.local.social.channel.lines(ChannelKind::Whisper).any(|l| l.author == "Bea" && l.text == "vale, hasta luego"));
    assert!(ana.game.local.social.channel.lines(ChannelKind::Yell).any(|l| l.author == "Bea"));

    // (The merged console, `Feed`, is unit-tested in client-core: this crate cannot name it.)
    let h: Vec<_> = ana.game.local.social.channel.history.iter().map(|l| (l.author.clone(), l.target.clone(), l.text.clone())).collect();
    assert!(h.contains(&("Ana".into(), "Bea".into(), "nos vemos luego".into())), "the echo of the own whisper: {h:?}");

    // Channel commands used wrongly never reach the server.
    let sent = ana.game.submit_chat("/g");
    assert!(!sent && console_has_game(&ana, "Uso: /g"));
    // Party and guild channels refuse non-members (the server says so).
    assert!(ana.game.submit_chat("/g hola grupo"));
    assert!(ana.game.submit_chat("/c hola clan"));
    settle(&s, &mut [&mut ana, &mut bea]);
    assert!(ana.game.local.social.channel.lines(ChannelKind::Party).next().is_none());
    assert!(ana.game.local.social.channel.lines(ChannelKind::Guild).next().is_none());
}

fn console_has_game(b: &Bot, needle: &str) -> bool {
    b.game.console.iter().any(|(t, _)| t.contains(needle))
}

#[test]
fn party_actions_invite_accept_kick_and_leave() {
    let s = field();
    let (mut ana, mut bea) = (s.bot("Ana"), s.bot("Bea"));
    bea.wait_until("seguro de grupo", |b| b.log().iter().any(|m| matches!(m, ao_protocol::S2C::Party(ao_protocol::PartyS2C::Safe { .. }))));
    bea.game.party_toggle_safe();
    bea.wait_until("seguro apagado", |b| !b.game.local.social.party.safe);
    let (aid, bid) = (ana.id(), bea.id());

    assert!(!ana.game.party_invite(aid), "not yourself");
    let (x, y) = bea.pos();
    ana.pump();
    assert_eq!(ana.game.player_at(x, y), Some(bid), "the player on a tile");
    assert!(ana.game.party_invite(bid));
    bea.wait_until("invitación", |b| b.game.local.social.party.invitation.is_some());
    bea.game.party_respond(true);
    assert!(bea.game.local.social.party.invitation.is_none(), "the notice goes away at once");
    ana.wait_until("grupo formado", |b| b.game.local.social.party.members.len() == 2);
    assert!(ana.game.is_party_leader() && !bea.game.is_party_leader());
    assert!(!ana.game.party_invite(bid), "already in the party");

    // Party channel now works.
    assert!(ana.game.submit_chat("/g a la carga"));
    bea.wait_until("canal de grupo", |b| b.game.local.social.channel.lines(ChannelKind::Party).any(|l| l.text == "a la carga"));

    ana.game.party_kick(bid);
    bea.wait_until("expulsado", |b| !b.game.local.social.party.in_party());
    ana.game.party_leave();
    ana.wait_until("grupo disuelto", |b| !b.game.local.social.party.in_party());
}

// ---------------------------------------------------------------- guild

fn db_path() -> PathBuf {
    let p = std::env::temp_dir().join(format!("ao_rs_p3_social_{}.db", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
    }
    p
}

/// (name, level, leadership, gold, gems)
fn seed(path: &PathBuf, pjs: &[(&str, u32, u8, u32, bool)]) {
    let db = Db::open(path.to_str().unwrap()).unwrap();
    let raw = rusqlite::Connection::open(path).unwrap();
    for &(name, level, leadership, gold, gems) in pjs {
        let acc = db.create_account(&name.to_lowercase(), PASS).unwrap();
        let c = db.create_character(acc.id, &NewCharacter { name, race: 1, class: 3, gender: 1, body: 1, head: 1, hp: 30, mana: 0, map: CITY.0, x: CITY.1, y: CITY.2 }).unwrap();
        let mut skills = [0u8; 24];
        skills[SKILL_LEADERSHIP] = leadership;
        let skills = skills.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",");
        raw.execute("UPDATE character SET level = ?1, skills = ?2, gold = ?3 WHERE id = ?4", rusqlite::params![level, skills, gold, c.id]).unwrap();
        if gems {
            for (i, g) in FOUNDATION_GEMS.iter().enumerate() {
                raw.execute("INSERT INTO inventory_item (character_id, slot, obj, amount) VALUES (?1, ?2, ?3, 1)", rusqlite::params![c.id, i as i64 + 1, g]).unwrap();
            }
        }
    }
}

fn enter(s: &TestServer, name: &str) -> Bot {
    let mut b = s.connect();
    assert!(b.account_login(&name.to_lowercase(), PASS), "login {name}");
    b.play(name);
    b.wait_until("en el mundo", |b| b.game.me().is_some());
    b
}

#[test]
fn guild_window_actions_found_apply_accept_news_kick() {
    let path = db_path();
    seed(&path, &[("Lider", 23, 50, 5000, true), ("Amigo", 20, 0, 3000, false), ("Novato", 1, 0, 0, false)]);
    let p = path.to_string_lossy().into_owned();
    let s = TestServer::with(move |st| {
        st.npcs = false;
        st.start = Some(CITY);
        st.config.database = p;
    });
    let (mut lider, mut amigo) = (enter(&s, "Lider"), enter(&s, "Amigo"));
    // Requirements as the window shows them: the leader meets all, the friend none.
    lider.sync();
    assert_eq!(lider.game.found_refusal(0), None);
    assert!(lider.game.found_requirements().iter().all(|r| r.1));
    assert!(amigo.game.found_refusal(0).is_some());

    // Local checks keep bad requests off the wire.
    assert!(!lider.game.guild_found("Nombre--mal", "x", 0));
    assert!(!lider.game.guild_found("La Hermandad", "  ", 0));
    assert!(!lider.game.guild_request_join("", "hola"));
    assert!(!lider.game.guild_reject("Amigo", " "));

    assert!(lider.game.guild_found("La Hermandad", "un clan de prueba", 0));
    lider.wait_until("clan fundado", |b| b.game.my_guild_name() == Some("La Hermandad") && b.game.is_guild_leader());
    // List as the window asks for it.
    amigo.game.guild_list();
    amigo.wait_until("lista de clanes", |b| b.game.local.social.guild.list.iter().any(|(n, _)| n == "La Hermandad"));

    assert!(amigo.game.guild_request_join("La Hermandad", "quiero entrar"));
    lider.wait_until("aspirante", |b| b.game.local.social.guild.info.as_ref().is_some_and(|g| g.applicants.iter().any(|a| a.name == "Amigo")));
    lider.game.guild_accept("Amigo");
    amigo.wait_until("miembro", |b| b.game.my_guild_name() == Some("La Hermandad") && !b.game.is_guild_leader());

    // Channel of the guild through the typed command, news by the leader.
    assert!(lider.game.submit_chat("/c bienvenido"));
    amigo.wait_until("canal de clan", |b| b.game.local.social.channel.lines(ChannelKind::Guild).any(|l| l.text == "bienvenido"));
    lider.game.guild_update_news("Reunión el sábado");
    amigo.wait_until("noticias", |b| b.game.local.social.guild.info.as_ref().is_some_and(|g| g.news == "Reunión el sábado"));
    assert!(lider.game.guild_update_description("nueva descripción"));
    amigo.wait_until("descripción", |b| b.game.local.social.guild.info.as_ref().is_some_and(|g| g.description == "nueva descripción"));

    lider.game.guild_kick("Amigo");
    amigo.wait_until("expulsado", |b| b.game.my_guild_name().is_none());
}

// ---------------------------------------------------------------- quests

#[test]
fn quest_offer_is_accepted_and_abandoned_from_the_window_actions() {
    let s = TestServer::start_at(1, 54, 58);
    let mut b = s.bot("Novato");
    s.advance(100);
    b.sync();
    let me = b.pos();
    let (_, x, y) = b.creatures().filter(|(_, e)| e.name.starts_with("Makrom")).map(|(id, e)| (id, e.x, e.y)).min_by_key(|(id, x, y)| (ao_rules::tile_distance(me, (*x, *y)), *id)).expect("Makrom");
    b.send(&ao_protocol::C2S::Interact { tx: x, ty: y });
    b.wait_until("oferta", |b| b.game.local.social.quest.offer.is_some());
    assert_eq!(b.game.quest_missing_details(), vec![]);

    b.game.quest_accept(407);
    b.wait_until("misión activa", |b| b.game.local.social.quest.active == vec![407]);
    // The server sends the details itself; asking again for a missing slot is harmless.
    b.wait_until("detalles", |b| b.game.local.social.quest.quest_in(1).is_some());
    assert!(b.game.quest_missing_details().is_empty());
    b.game.quest_list();
    b.game.quest_details(1);
    b.sync();
    assert_eq!(b.game.local.social.quest.active, vec![407]);

    b.game.quest_abandon(1);
    b.wait_until("abandonada", |b| b.game.local.social.quest.active.is_empty());
    assert!(console_has(&b, "Has abandonado la misión"));
}
