//! P2 exit scenario: two account characters in Ullathorpe found a guild and join it,
//! talk on the guild channel, form a party and talk on the party channel, trade gold
//! with the double confirmation, and after reconnecting the guild is still theirs.
//! Quests, factions and party experience have their own scenarios (p2_quest, p2_faction,
//! p2_party); this one checks the systems working together.
use ao_protocol::social::channel::ChannelKind;
use ao_protocol::{ChannelC2S, GuildC2S, PartyC2S, TradeC2S, C2S};
use ao_rules::guild::{Alignment, FOUNDATION_GEMS, SKILL_LEADERSHIP};
use ao_rules::{Dir, WALK_MS};
use ao_testkit::{Bot, TestServer};
use server::db::{Db, NewCharacter};
use std::path::{Path, PathBuf};

const CITY: (u16, i32, i32) = (1, 50, 50);
const PASS: &str = "secreta1";

fn db_path() -> PathBuf {
    let p = std::env::temp_dir().join(format!("ao_rs_p2_exit_{}.db", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
    }
    p
}

/// (name, level, leadership, gold, foundation gems)
fn seed(path: &Path, pjs: &[(&str, u32, u8, u32, bool)]) {
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

fn server(path: &Path) -> TestServer {
    let path = path.to_str().unwrap().to_string();
    TestServer::with(move |st| {
        st.npcs = false;
        st.start = Some(CITY);
        st.config.database = path;
    })
}

fn enter(s: &TestServer, name: &str) -> Bot {
    let mut b = s.connect();
    assert!(b.account_login(&name.to_lowercase(), PASS), "login {name}");
    b.play(name);
    b.wait_until("en el mundo", |b| b.game.me().is_some());
    b
}

fn sync(a: &mut Bot, b: &mut Bot) {
    a.sync();
    b.sync();
    a.sync();
}

fn heard(b: &Bot, kind: ChannelKind, text: &str) -> bool {
    b.game.local.social.channel.history.iter().any(|l| l.channel == kind && l.text == text)
}

/// Walks `b` next to `a` (orthogonally adjacent).
fn approach(s: &TestServer, b: &mut Bot, a: &Bot) {
    for _ in 0..200 {
        let (me, to) = (b.pos(), a.pos());
        if (me.0 - to.0).abs() + (me.1 - to.1).abs() <= 1 {
            return;
        }
        let dir = Dir::towards(to.0 - me.0, to.1 - me.1);
        if !b.walk(dir) {
            // Blocked: try the other axis.
            let alt = if matches!(dir, Dir::Left | Dir::Right) { Dir::towards(0, to.1 - me.1) } else { Dir::towards(to.0 - me.0, 0) };
            b.walk(alt);
        }
        s.advance(WALK_MS);
        b.sync();
    }
    panic!("no llega junto a {:?} (esta en {:?})", a.pos(), b.pos());
}

#[test]
fn guild_party_channels_and_trade_together() {
    let path = db_path();
    seed(&path, &[("Lider", 23, 50, 5000, true), ("Amigo", 20, 0, 3000, false)]);
    {
        let s = server(&path);
        let mut lider = enter(&s, "Lider");
        let mut amigo = enter(&s, "Amigo");
        approach(&s, &mut amigo, &lider);

        // ---- guild: found, apply, accept (500 gold of the leader, Clanes.dat level 1).
        lider.send(&C2S::Guild(GuildC2S::Found { name: "La Hermandad".into(), description: "codex".into(), alignment: Alignment::Neutral as u8 }));
        sync(&mut lider, &mut amigo);
        assert!(lider.game.local.social.guild.info.is_some(), "clan fundado");
        amigo.send(&C2S::Guild(GuildC2S::RequestJoin { guild: "La Hermandad".into(), application: "quiero entrar".into() }));
        sync(&mut lider, &mut amigo);
        let gold0 = lider.game.local.gold;
        lider.send(&C2S::Guild(GuildC2S::Accept { name: "Amigo".into() }));
        amigo.wait_until("miembro del clan", |b| b.game.local.social.guild.info.is_some());
        lider.sync();
        assert!(lider.game.local.gold < gold0, "aceptar cuesta oro");

        // ---- guild channel reaches the other member.
        lider.send(&C2S::Channel(ChannelC2S::Guild { text: "bienvenido".into() }));
        amigo.wait_until("oye el canal de clan", |b| heard(b, ChannelKind::Guild, "bienvenido"));

        // ---- party: the guest switches off the party seguro, the leader invites.
        amigo.send(&C2S::Party(PartyC2S::ToggleSafe));
        amigo.wait_until("seguro de grupo apagado", |b| !b.game.local.social.party.safe);
        let aid = amigo.id();
        lider.send(&C2S::Party(PartyC2S::Invite { target: aid }));
        amigo.wait_until("invitacion", |b| b.game.local.social.party.invitation.is_some());
        amigo.send(&C2S::Party(PartyC2S::Respond { accept: true }));
        lider.wait_until("grupo formado", |b| b.game.local.social.party.members.len() == 2);
        amigo.send(&C2S::Channel(ChannelC2S::Party { text: "vamos".into() }));
        lider.wait_until("oye el canal de grupo", |b| heard(b, ChannelKind::Party, "vamos"));

        // ---- trade: 200 gold from the guest, nothing back, double confirmation.
        let lid = lider.id();
        amigo.send(&C2S::Trade(TradeC2S::Propose { target: lid }));
        lider.wait_until("invitado a comerciar", |b| b.game.local.social.trade.invited_by.is_some());
        lider.send(&C2S::Trade(TradeC2S::Propose { target: aid }));
        amigo.wait_until("comercio abierto", |b| b.game.local.social.trade.player.is_some());
        lider.wait_until("comercio abierto", |b| b.game.local.social.trade.player.is_some());
        let (lg, ag) = (lider.game.local.gold, amigo.game.local.gold);
        amigo.send(&C2S::Trade(TradeC2S::OfferGold { amount: 200 }));
        lider.wait_until("ve la oferta", |b| b.game.local.social.trade.player.as_ref().is_some_and(|t| t.theirs.gold == 200));
        amigo.sync();
        let rev = lider.game.local.social.trade.player.as_ref().unwrap().rev;
        for b in [&mut lider, &mut amigo] {
            b.send(&C2S::Trade(TradeC2S::Confirm { rev }));
        }
        sync(&mut lider, &mut amigo);
        for b in [&mut lider, &mut amigo] {
            b.send(&C2S::Trade(TradeC2S::Accept { rev }));
        }
        lider.wait_until("oro recibido", |b| b.game.local.gold == lg + 200);
        amigo.wait_until("oro entregado", |b| b.game.local.gold == ag - 200);
        assert!(lider.kicked().is_none() && amigo.kicked().is_none());
    }

    // ---- persistence: a new server on the same database still has the guild.
    let s = server(&path);
    let mut lider = enter(&s, "Lider");
    lider.wait_until("clan cargado", |b| b.game.local.social.guild.info.as_ref().is_some_and(|g| g.name == "La Hermandad"));
}
