//! P2 guilds: found a guild with the official requirements (level 23, 50 leadership, the
//! four foundation gems), applications, accept (costs gold, capacity by guild level),
//! reject, kick, leave, news, relations between guilds, persistence across a restart, and
//! the cheats: founding without requirements, acting without being the leader, kicking the
//! founder, acting on another guild, invalid or duplicate names, flood.
use ao_protocol::social::guild::GuildRelation;
use ao_protocol::{GuildC2S, GuildS2C, C2S, S2C};
use ao_rules::guild::{Alignment, FOUNDATION_GEMS, SKILL_LEADERSHIP};
use ao_testkit::{Bot, TestServer};
use server::db::{Db, NewCharacter};
use std::path::PathBuf;

const START: (u16, i32, i32) = (1, 50, 50);
const PASS: &str = "secreta1";

/// A character prepared in the database before the server starts.
struct Pj {
    name: &'static str,
    level: u32,
    leadership: u8,
    gold: u32,
    gems: bool,
}

fn pj(name: &'static str, level: u32) -> Pj {
    Pj { name, level, leadership: 0, gold: 0, gems: false }
}

fn founder(name: &'static str) -> Pj {
    Pj { name, level: 23, leadership: 50, gold: 5000, gems: true }
}

fn db_path(test: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ao_rs_p2_guild_{test}_{}.db", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", p.display()));
    }
    p
}

/// Creates the accounts and characters (one account each, user = lowercase name).
fn seed(path: &PathBuf, pjs: &[Pj]) -> Db {
    let db = Db::open(path.to_str().unwrap()).unwrap();
    let raw = rusqlite::Connection::open(path).unwrap();
    for p in pjs {
        let acc = db.create_account(&p.name.to_lowercase(), PASS).unwrap();
        let c = db.create_character(acc.id, &NewCharacter { name: p.name, race: 1, class: 3, gender: 1, body: 1, head: 1, hp: 30, mana: 0, map: START.0, x: START.1, y: START.2 }).unwrap();
        let mut skills = [0u8; 24];
        skills[SKILL_LEADERSHIP] = p.leadership;
        let skills = skills.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",");
        raw.execute("UPDATE character SET level = ?1, skills = ?2, gold = ?3 WHERE id = ?4", rusqlite::params![p.level, skills, p.gold, c.id]).unwrap();
        if p.gems {
            for (i, g) in FOUNDATION_GEMS.iter().enumerate() {
                raw.execute("INSERT INTO inventory_item (character_id, slot, obj, amount) VALUES (?1, ?2, ?3, 1)", rusqlite::params![c.id, i as i64 + 1, g]).unwrap();
            }
        }
    }
    db
}

fn server(path: &std::path::Path) -> TestServer {
    let path = path.to_str().unwrap().to_string();
    TestServer::with(move |st| {
        st.npcs = false;
        st.start = Some(START);
        st.config.database = path;
    })
}

fn enter(s: &TestServer, name: &str) -> Bot {
    let mut b = s.connect();
    assert!(b.account_login(&name.to_lowercase(), PASS), "login {name}");
    b.play(name);
    b
}

fn guild(b: &mut Bot, msg: GuildC2S) {
    b.send(&C2S::Guild(msg));
    b.sync();
}

fn saw(b: &Bot, text: &str) -> bool {
    b.console().any(|l| l.contains(text))
}

fn info(b: &Bot) -> Option<&ao_protocol::social::guild::GuildInfo> {
    b.game.local.social.guild.info.as_ref()
}

fn found(b: &mut Bot, name: &str) {
    guild(b, GuildC2S::Found { name: name.into(), description: "codex".into(), alignment: Alignment::Neutral as u8 });
}

fn join(b: &mut Bot, g: &str) {
    guild(b, GuildC2S::RequestJoin { guild: g.into(), application: "quiero entrar".into() });
}

fn accept(b: &mut Bot, name: &str) {
    guild(b, GuildC2S::Accept { name: name.into() });
}

fn sync_all(bots: &mut [&mut Bot]) {
    for b in bots {
        b.sync();
    }
}

#[test]
fn found_join_accept_reject_kick_leave_and_persist() {
    let path = db_path("flow");
    drop(seed(&path, &[founder("Lider"), pj("Uno", 20), pj("Dos", 20)]));
    {
        let s = server(&path);
        let mut lider = enter(&s, "Lider");
        let mut uno = enter(&s, "Uno");
        let mut dos = enter(&s, "Dos");

        found(&mut lider, "Los Pibes");
        uno.sync();
        let i = info(&lider).expect("clan fundado");
        assert_eq!((i.name.as_str(), i.level, i.leader.as_str(), i.founder.as_str(), i.max_members), ("Los Pibes", 1, "Lider", "Lider", 3));
        assert!(i.news.contains("Bienvenido a Los Pibes"), "{}", i.news);
        assert!(lider.game.local.inventory.iter().flatten().all(|s| !FOUNDATION_GEMS.contains(&s.obj)), "las gemas se consumen");
        assert!(saw(&uno, "Lider ha fundado el clan <Los Pibes> de alineación Neutral."));
        assert_eq!(uno.game.local.social.guild.tag_of(lider.id()), Some("Los Pibes"), "los demás ven el clan bajo el nombre");

        join(&mut uno, "los pibes");
        lider.sync();
        assert!(saw(&uno, "Tu solicitud ha sido enviada"));
        assert!(saw(&lider, "[Uno] ha enviado solicitud"));
        assert_eq!(info(&lider).unwrap().applicants[0].application, "quiero entrar");

        let gold = lider.game.local.gold;
        assert_eq!(gold, 5000);
        accept(&mut lider, "uno");
        uno.sync();
        assert_eq!(lider.game.local.gold, gold - 500, "PriceAcceptMemberGuildLevel1");
        assert_eq!(info(&uno).map(|i| i.name.as_str()), Some("Los Pibes"));
        assert!(info(&uno).unwrap().applicants.is_empty(), "solo el líder ve los aspirantes");
        assert_eq!(info(&lider).unwrap().members.len(), 2);
        assert!(saw(&uno, "[Uno] ha sido aceptado como miembro del clan."));

        join(&mut dos, "Los Pibes");
        guild(&mut lider, GuildC2S::Reject { name: "Dos".into(), reason: "no hay lugar".into() });
        dos.sync();
        assert!(saw(&dos, "Fue rechazada tu solicitud de ingreso a Los Pibes : no hay lugar"));
        assert!(info(&lider).unwrap().applicants.is_empty());

        guild(&mut lider, GuildC2S::UpdateNews { news: "Junta en Ulla".into() });
        uno.sync();
        assert_eq!(info(&uno).unwrap().news, "Junta en Ulla");

        guild(&mut uno, GuildC2S::Leave);
        lider.sync();
        assert!(info(&uno).is_none() && saw(&uno, "Dejas el clan."));
        assert!(saw(&lider, "Uno deja el clan."));
        assert_eq!(lider.game.local.social.guild.tag_of(uno.id()), None);

        join(&mut uno, "Los Pibes");
        accept(&mut lider, "Uno");
        guild(&mut lider, GuildC2S::Kick { name: "UNO".into() });
        uno.sync();
        assert!(info(&uno).is_none() && saw(&uno, "Has sido expulsado del clan."));

        guild(&mut lider, GuildC2S::Leave);
        assert!(saw(&lider, "Tu no puedes salir de ningún clan."), "el fundador no puede irse");

        join(&mut uno, "Los Pibes");
        accept(&mut lider, "Uno");
        // Offline rejection: Dos applies, logs out, is rejected, sees it on the next login.
        join(&mut dos, "Los Pibes");
        drop(dos);
        lider.wait_until("Dos se desconecta", |b| b.console().any(|l| l == "Dos se desconecto."));
        guild(&mut lider, GuildC2S::Reject { name: "Dos".into(), reason: "otra vez".into() });
    }
    // Restart on the same database: the guild, its member and the rejection are back.
    let s = server(&path);
    let lider = enter(&s, "Lider");
    let i = info(&lider).expect("clan cargado de la base");
    assert_eq!((i.name.as_str(), i.news.as_str()), ("Los Pibes", "Junta en Ulla"));
    let names: Vec<&str> = i.members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"Lider") && names.contains(&"Uno"), "{names:?}");
    assert!(i.members.iter().any(|m| m.name == "Lider" && m.online && m.founder && m.leader));
    assert!(i.members.iter().any(|m| m.name == "Uno" && !m.online && !m.leader));
    let dos = enter(&s, "Dos");
    assert!(saw(&dos, "Fue rechazada tu solicitud de ingreso a Los Pibes : otra vez"));
    let uno = enter(&s, "Uno");
    assert_eq!(info(&uno).map(|i| i.name.as_str()), Some("Los Pibes"));
}

#[test]
fn cheats_are_refused() {
    let path = db_path("cheats");
    let db = seed(
        &path,
        &[founder("Lider"), founder("Rival"), Pj { leadership: 49, ..founder("Flojo") }, Pj { gems: false, ..founder("Pobre") }, pj("Uno", 20), pj("Dos", 20), pj("Tres", 20), pj("Novato", 12)],
    );
    let s = server(&path);
    let mut lider = enter(&s, "Lider");
    let mut rival = enter(&s, "Rival");
    let mut flojo = enter(&s, "Flojo");
    let mut pobre = enter(&s, "Pobre");
    let mut uno = enter(&s, "Uno");
    let mut dos = enter(&s, "Dos");
    let mut tres = enter(&s, "Tres");
    let mut novato = enter(&s, "Novato");

    // Founding without the requirements.
    found(&mut uno, "Sin Nivel");
    found(&mut flojo, "Sin Liderazgo");
    found(&mut pobre, "Sin Gemas");
    for b in [&uno, &flojo, &pobre] {
        assert!(saw(b, "Para fundar un clan debes ser Nivel 23"), "{:?}", b.console().collect::<Vec<_>>());
        assert!(info(b).is_none());
    }
    // Names: invalid and duplicated (case-insensitive).
    for bad in ["", "Los Pibes 2", "Ñoños", "a b  c", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", "<script>"] {
        found(&mut lider, bad);
    }
    assert_eq!(lider.console().filter(|l| *l == "Nombre de clan inválido.").count(), 6);
    found(&mut lider, "Los Pibes");
    found(&mut rival, "LOS PIBES");
    assert!(saw(&rival, "Ya existe un clan con ese nombre."));
    found(&mut lider, "Otro Clan");
    assert!(saw(&lider, "Ya perteneces a un clan, no podés fundar otro."));
    found(&mut rival, "Rivales");
    assert!(info(&rival).is_some());

    s.advance(2000);
    // Applications: newbie, unknown guild, twice.
    join(&mut novato, "Los Pibes");
    assert!(saw(&novato, "Los newbies no tienen derecho a entrar a un clan."));
    join(&mut uno, "No Existe");
    assert!(saw(&uno, "Ese clan no existe."));
    join(&mut uno, "Los Pibes");
    join(&mut uno, "Los Pibes");
    assert!(saw(&uno, "Ya has enviado una solicitud a este clan."));
    join(&mut dos, "Los Pibes");
    join(&mut tres, "Los Pibes");
    sync_all(&mut [&mut lider]);

    s.advance(2000);
    // Not the leader / not in a guild.
    accept(&mut uno, "Dos");
    assert!(saw(&uno, "No perteneces a ningún clan."));
    accept(&mut rival, "Dos");
    assert!(saw(&rival, "El Pj no es aspirante al clan."), "el líder de otro clan no ve los aspirantes ajenos");
    accept(&mut lider, "Novato");
    assert!(saw(&lider, "El Pj no es aspirante al clan."));
    accept(&mut lider, "Uno");
    guild(&mut uno, GuildC2S::Accept { name: "Dos".into() });
    guild(&mut uno, GuildC2S::Reject { name: "Dos".into(), reason: "x".into() });
    guild(&mut uno, GuildC2S::UpdateNews { news: "hackeado".into() });
    guild(&mut uno, GuildC2S::UpdateDescription { description: "hackeado".into() });
    assert_eq!(uno.console().filter(|l| *l == "No eres el líder de tu clan.").count(), 4);
    lider.sync();
    assert_ne!(info(&lider).unwrap().news, "hackeado");
    assert_eq!(info(&lider).unwrap().applicants.len(), 2, "Dos sigue siendo aspirante");

    s.advance(2000);
    // Kicking the founder, or a member of another guild.
    guild(&mut uno, GuildC2S::Kick { name: "Lider".into() });
    assert!(saw(&uno, "No podés expulsar ese personaje del clan."));
    guild(&mut rival, GuildC2S::Kick { name: "Uno".into() });
    assert!(saw(&rival, "No podés expulsar ese personaje del clan."));
    guild(&mut lider, GuildC2S::Kick { name: "Lider".into() });
    lider.sync();
    assert_eq!(info(&lider).unwrap().members.len(), 2, "nadie salió");

    // Capacity of a level 1 guild: 3 members (MembersByLevel1), founder included.
    accept(&mut lider, "Dos");
    accept(&mut lider, "Tres");
    assert!(saw(&lider, "La capacidad del clan está completa."));
    assert_eq!(info(&lider).unwrap().members.len(), 3);
    // Applying elsewhere withdraws the old application; then Los Pibes cannot take Tres.
    join(&mut tres, "Rivales");
    rival.sync();
    accept(&mut rival, "Tres");
    tres.sync();
    assert_eq!(info(&tres).map(|i| i.name.as_str()), Some("Rivales"));
    lider.sync();
    assert!(info(&lider).unwrap().applicants.is_empty(), "la solicitud vieja se retira");

    s.advance(2000);
    // Flood of applications: MAXASPIRANTES = 10 per guild.
    let mut extra = Vec::new();
    let names: Vec<String> = (0..11).map(|i| format!("Aspirante{}", (b'a' + i) as char)).collect();
    for n in &names {
        let acc = db.create_account(&n.to_lowercase(), PASS).unwrap();
        let c = db.create_character(acc.id, &NewCharacter { name: n, race: 1, class: 3, gender: 1, body: 1, head: 1, hp: 30, mana: 0, map: START.0, x: START.1, y: START.2 }).unwrap();
        rusqlite::Connection::open(&path).unwrap().execute("UPDATE character SET level = 20 WHERE id = ?1", [c.id]).unwrap();
        let mut b = enter(&s, n);
        join(&mut b, "Rivales");
        extra.push(b);
    }
    assert!(extra[..10].iter().all(|b| saw(b, "Tu solicitud ha sido enviada")));
    assert!(saw(&extra[10], "El clan tiene demasiados aspirantes."));
    rival.sync();
    assert_eq!(info(&rival).unwrap().applicants.len(), 10);

    // A made-up alignment is a cheat (strike), a flood of guild messages gets kicked.
    guild(&mut pobre, GuildC2S::Found { name: "Raro".into(), description: String::new(), alignment: 9 });
    assert!(info(&pobre).is_none());
    for _ in 0..40 {
        novato.send(&C2S::Guild(GuildC2S::List));
    }
    novato.wait_until("expulsado por flood", |b| b.closed());
    assert!(novato.kicked().is_some());
    assert!(!lider.closed());
}

#[test]
fn alliances_and_wars() {
    let path = db_path("relations");
    drop(seed(&path, &[founder("Lider"), founder("Rival"), founder("Tercero"), pj("Uno", 20)]));
    let s = server(&path);
    let mut lider = enter(&s, "Lider");
    let mut rival = enter(&s, "Rival");
    let mut tercero = enter(&s, "Tercero");
    let mut uno = enter(&s, "Uno");
    found(&mut lider, "Los Pibes");
    found(&mut rival, "Rivales");
    found(&mut tercero, "Terceros");
    join(&mut uno, "Los Pibes");
    accept(&mut lider, "Uno");
    uno.sync();

    let rel = |b: &Bot, g: &str| info(b).unwrap().relations.iter().find(|r| r.0 == g).map(|r| r.1);

    // Only the leader, never with the own guild.
    guild(&mut uno, GuildC2S::Propose { guild: "Rivales".into(), relation: GuildRelation::Alliance, text: String::new() });
    assert!(saw(&uno, "No eres el líder de tu clan."));
    guild(&mut lider, GuildC2S::DeclareWar { guild: "los pibes".into() });
    assert!(saw(&lider, "No podés hacer eso con tu propio clan."));
    guild(&mut lider, GuildC2S::AcceptProposal { guild: "Rivales".into() });
    assert!(saw(&lider, "Ese clan no te ha propuesto nada."));

    // Alliance: proposal, the other leader accepts.
    guild(&mut lider, GuildC2S::Propose { guild: "Rivales".into(), relation: GuildRelation::Alliance, text: "juntos".into() });
    rival.sync();
    assert!(saw(&rival, "El clan Los Pibes propone una alianza a tu clan."));
    assert_eq!(info(&rival).unwrap().proposals, vec![("Los Pibes".to_string(), GuildRelation::Alliance, "juntos".to_string())]);
    guild(&mut tercero, GuildC2S::AcceptProposal { guild: "Los Pibes".into() });
    assert!(saw(&tercero, "Ese clan no te ha propuesto nada."), "aceptar la propuesta hecha a otro clan");
    guild(&mut rival, GuildC2S::AcceptProposal { guild: "Los Pibes".into() });
    guild(&mut rival, GuildC2S::AcceptProposal { guild: "Los Pibes".into() });
    assert!(saw(&rival, "Ese clan no te ha propuesto nada."), "aceptar dos veces");
    sync_all(&mut [&mut lider, &mut uno]);
    assert_eq!(rel(&lider, "Rivales"), Some(GuildRelation::Alliance));
    assert_eq!(rel(&uno, "Rivales"), Some(GuildRelation::Alliance));
    assert_eq!(rel(&rival, "Los Pibes"), Some(GuildRelation::Alliance));
    assert!(saw(&uno, "Tu clan ha firmado la alianza con Rivales."));

    // War is unilateral; peace has to be accepted.
    guild(&mut tercero, GuildC2S::DeclareWar { guild: "Los Pibes".into() });
    lider.sync();
    assert!(saw(&lider, "Terceros LE DECLARA LA GUERRA A TU CLAN"));
    assert!(saw(&tercero, "TU CLAN HA ENTRADO EN GUERRA CON Los Pibes"));
    assert_eq!(rel(&lider, "Terceros"), Some(GuildRelation::War));
    guild(&mut lider, GuildC2S::Propose { guild: "Rivales".into(), relation: GuildRelation::Peace, text: String::new() });
    assert!(saw(&lider, "Esa propuesta no cambia la relación"), "paz con quien no está en guerra");
    guild(&mut lider, GuildC2S::Propose { guild: "Terceros".into(), relation: GuildRelation::Peace, text: "basta".into() });
    guild(&mut tercero, GuildC2S::RejectProposal { guild: "Los Pibes".into() });
    lider.sync();
    assert!(saw(&lider, "El clan Terceros rechazó la propuesta de tu clan."));
    assert_eq!(rel(&lider, "Terceros"), Some(GuildRelation::War));
    guild(&mut lider, GuildC2S::Propose { guild: "Terceros".into(), relation: GuildRelation::Peace, text: String::new() });
    guild(&mut tercero, GuildC2S::AcceptProposal { guild: "Los Pibes".into() });
    lider.sync();
    assert_eq!(rel(&lider, "Terceros"), None, "en paz");

    // The guild list.
    guild(&mut uno, GuildC2S::List);
    assert_eq!(uno.game.local.social.guild.list.len(), 3);
    assert!(uno.log().iter().any(|m| matches!(m, S2C::Guild(GuildS2C::List(_)))));
}
