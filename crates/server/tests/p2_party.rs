//! P2 parties (ModGrupos.bas, CalcularDarExpGrupal) on ao-testkit: invite, accept, leave,
//! kick, disband when the leader goes, experience shared with the members nearby, and the
//! cheats around it (answering invitations that do not exist, kicking without leading,
//! two parties at once, inviting with the guest's seguro on...).
use ao_formats::recursos_dir;
use ao_protocol::social::party::PartyMember;
use ao_protocol::{FactionC2S, PartyC2S, C2S};
use ao_rules::combat::ATTACK_INTERVAL_MS;
use ao_rules::{tile_distance, Dir};
use ao_testkit::{Bot, TestServer};

/// Map 2: not a safe zone.
const FIELD: (u16, i32, i32) = (2, 50, 50);
const WARRIOR: u8 = 3;

fn server() -> TestServer {
    TestServer::with(|st| {
        st.npcs = false;
        st.start = Some(FIELD);
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = FIELD;
        b.creatures_per_map = 0;
    })
}

/// A dev-login bot with its party "seguro" off (so it can be invited).
fn open(s: &TestServer, name: &str) -> Bot {
    let mut b = s.bot(name);
    b.wait_until("seguro de grupo", |b| b.log().iter().any(|m| matches!(m, ao_protocol::S2C::Party(ao_protocol::PartyS2C::Safe { .. }))));
    party(&mut b, PartyC2S::ToggleSafe);
    b.wait_until("seguro apagado", |b| !b.game.local.social.party.safe);
    b
}

fn party(b: &mut Bot, m: PartyC2S) {
    b.send(&C2S::Party(m));
}

fn says(b: &Bot, text: &str) -> bool {
    b.console().any(|t| t.contains(text))
}

fn members(b: &Bot) -> Vec<u32> {
    b.game.local.social.party.members.iter().map(|m: &PartyMember| m.id).collect()
}

fn sync_all(s: &TestServer, bots: &mut [&mut Bot]) {
    s.settle();
    for b in bots.iter_mut() {
        b.sync();
    }
}

/// `leader` invites `guest` and `guest` accepts.
fn join(s: &TestServer, leader: &mut Bot, guest: &mut Bot) {
    let gid = guest.id();
    party(leader, PartyC2S::Invite { target: gid });
    let lid = leader.id();
    guest.wait_until("invitacion", |b| b.game.local.social.party.invitation.as_ref().is_some_and(|(f, _)| *f == lid));
    party(guest, PartyC2S::Respond { accept: true });
    guest.wait_until("en el grupo", |b| b.game.local.social.party.has(gid));
    leader.wait_until("lider ve al nuevo", |b| b.game.local.social.party.has(gid));
    s.settle();
}

#[test]
fn invite_accept_leave_and_kick() {
    let s = server();
    let mut ana = s.bot("Ana");
    let mut beto = s.bot("Beto");
    let (aid, bid) = (ana.id(), beto.id());

    // Beto starts with his party seguro on: nobody can invite him.
    party(&mut ana, PartyC2S::Invite { target: bid });
    sync_all(&s, &mut [&mut ana, &mut beto]);
    assert!(says(&ana, "desactivar el seguro de grupos"));
    assert!(beto.game.local.social.party.invitation.is_none());

    party(&mut beto, PartyC2S::ToggleSafe);
    sync_all(&s, &mut [&mut ana, &mut beto]);
    assert!(!beto.game.local.social.party.safe);
    join(&s, &mut ana, &mut beto);
    assert!(says(&ana, "¡El grupo ha sido creado!"));
    assert_eq!(members(&ana), vec![aid, bid]);
    assert_eq!(ana.game.local.social.party.leader, aid);
    assert!(says(&beto, "¡Has sido añadido al grupo!"));

    // Party members cannot hurt each other (PuedeAtacar). The first two bots spawn next
    // to each other; Beto steps into Ana's row or column if diagonal.
    let (pa, pb) = (ana.pos(), beto.pos());
    assert_eq!(tile_distance(pa, pb), 1, "spawned next to each other");
    if pa.0 != pb.0 && pa.1 != pb.1 {
        assert!(beto.walk(Dir::towards(pa.0 - pb.0, 0)));
        s.advance(ao_rules::WALK_MS);
        beto.sync();
        ana.sync();
    }
    let (pa, pb) = (ana.pos(), beto.pos());
    ana.send(&C2S::Turn { dir: Dir::towards(pb.0 - pa.0, pb.1 - pa.1) });
    let hp = beto.hp();
    let seq = ana.game.next_seq();
    ana.send(&C2S::Melee { seq });
    s.advance(ATTACK_INTERVAL_MS);
    sync_all(&s, &mut [&mut ana, &mut beto]);
    assert!(says(&ana, "No podés atacar a un miembro de tu grupo."));
    assert_eq!(beto.hp(), hp);

    let mut caro = open(&s, "Caro");
    let cid = caro.id();
    join(&s, &mut ana, &mut caro);
    sync_all(&s, &mut [&mut ana, &mut beto, &mut caro]);
    assert_eq!(members(&beto), vec![aid, bid, cid]);
    assert!(says(&beto, "Caro ha ingresado al grupo."));

    // A member leaves: the rest see it.
    party(&mut caro, PartyC2S::Leave);
    sync_all(&s, &mut [&mut ana, &mut beto, &mut caro]);
    assert!(!caro.game.local.social.party.in_party());
    assert_eq!(members(&ana), vec![aid, bid]);
    assert!(says(&ana, "Caro ha abandonado el grupo."));

    // The leader kicks the last one: the party disbands.
    party(&mut ana, PartyC2S::Kick { member: bid });
    sync_all(&s, &mut [&mut ana, &mut beto]);
    assert!(!beto.game.local.social.party.in_party());
    assert!(!ana.game.local.social.party.in_party());
    assert!(says(&beto, "Has sido removido del grupo."));
    assert!(says(&ana, "El grupo se ha disuelto"));
}

#[test]
fn the_leader_leaving_disbands_and_disconnects_clean_up() {
    let s = server();
    let mut ana = s.bot("Ana");
    let mut beto = open(&s, "Beto");
    let mut caro = open(&s, "Caro");
    let bid = beto.id();
    join(&s, &mut ana, &mut beto);
    join(&s, &mut ana, &mut caro);

    // Leader leaves on purpose (HandleAbandonarGrupo -> FinalizarGrupo).
    party(&mut ana, PartyC2S::Leave);
    sync_all(&s, &mut [&mut ana, &mut beto, &mut caro]);
    for b in [&ana, &beto, &caro] {
        assert!(!b.game.local.social.party.in_party());
    }
    assert!(says(&beto, "El líder ha abandonado el grupo. El grupo se disuelve."));
    assert!(says(&ana, "Has disuelto el grupo."));

    // Again; now a member disconnects (SalirDeGrupoForzado) and then the leader does.
    join(&s, &mut ana, &mut beto);
    join(&s, &mut ana, &mut caro);
    drop(caro);
    s.advance(100);
    sync_all(&s, &mut [&mut ana, &mut beto]);
    assert_eq!(members(&ana), vec![ana.id(), bid]);
    drop(ana);
    s.advance(100);
    beto.sync();
    assert!(!beto.game.local.social.party.in_party());

    // A pending invitation dies with its sender: answering it later is refused.
    let mut dani = s.bot("Dani");
    party(&mut dani, PartyC2S::Invite { target: bid });
    let did = dani.id();
    beto.wait_until("invitacion de Dani", |b| b.game.local.social.party.invitation.as_ref().is_some_and(|(f, _)| *f == did));
    drop(dani);
    s.advance(100);
    party(&mut beto, PartyC2S::Respond { accept: true });
    s.settle();
    beto.sync();
    assert!(!beto.game.local.social.party.in_party());
    assert!(says(&beto, "Solicitud de grupo invalida"));
}

#[test]
fn cheats_and_limits() {
    let s = server();
    let mut ana = s.bot("Ana");
    let mut beto = open(&s, "Beto");
    let mut caro = open(&s, "Caro");
    let (aid, bid, cid) = (ana.id(), beto.id(), caro.id());

    // Answering an invitation nobody sent.
    party(&mut caro, PartyC2S::Respond { accept: true });
    // Leaving, kicking without a party; inviting oneself or nobody.
    party(&mut caro, PartyC2S::Leave);
    party(&mut caro, PartyC2S::Kick { member: aid });
    party(&mut ana, PartyC2S::Invite { target: aid });
    party(&mut ana, PartyC2S::Invite { target: 999_999 });
    sync_all(&s, &mut [&mut ana, &mut caro]);
    assert!(says(&caro, "Solicitud de grupo invalida"));
    assert!(says(&caro, "No estás en ningún grupo."));
    assert!(says(&ana, "No hay ningún usuario ahí."));
    assert!(!caro.game.local.social.party.in_party());

    join(&s, &mut ana, &mut beto);
    // Accepting twice does nothing more.
    party(&mut beto, PartyC2S::Respond { accept: true });
    // A member cannot kick nor invite.
    party(&mut beto, PartyC2S::Kick { member: aid });
    party(&mut beto, PartyC2S::Invite { target: cid });
    // The leader cannot kick himself or a stranger.
    party(&mut ana, PartyC2S::Kick { member: aid });
    party(&mut ana, PartyC2S::Kick { member: cid });
    sync_all(&s, &mut [&mut ana, &mut beto, &mut caro]);
    assert!(says(&beto, "Solo el líder puede echar"));
    assert!(says(&beto, "debe hacerlo Ana"));
    assert!(says(&ana, "No podés expulsarte a ti mismo."));
    assert!(says(&ana, "Ese usuario no está en tu grupo."));
    assert!(caro.game.local.social.party.invitation.is_none());
    assert_eq!(members(&ana), vec![aid, bid]);

    // Two parties at once: Caro forms her own and Ana tries to recruit her.
    let mut dani = open(&s, "Dani");
    join(&s, &mut caro, &mut dani);
    party(&mut ana, PartyC2S::Invite { target: cid });
    party(&mut ana, PartyC2S::Invite { target: dani.id() });
    sync_all(&s, &mut [&mut ana, &mut caro]);
    assert!(says(&ana, "ya se encuentra en un grupo"));
    assert!(caro.game.local.social.party.invitation.is_none());

    // Different factions cannot party: Eva turns criminal (HandleLeaveFaction).
    let mut eva = open(&s, "Eva");
    eva.send(&C2S::Faction(FactionC2S::Leave { npc: None }));
    eva.wait_until("criminal", |b| b.game.local.social.faction.status == 0 && b.game.local.social.faction.known);
    party(&mut ana, PartyC2S::Invite { target: eva.id() });
    sync_all(&s, &mut [&mut ana, &mut eva]);
    assert!(says(&ana, "diferentes facciones"));

    // Six at most (Miembros(1 To 6)).
    let mut more: Vec<Bot> = ["Fede", "Gabi", "Hugo", "Ines", "Juan"].iter().map(|n| open(&s, n)).collect();
    for b in more.iter_mut().take(4) {
        join(&s, &mut ana, b);
    }
    assert_eq!(members(&ana).len(), 6);
    let jid = more[4].id();
    party(&mut ana, PartyC2S::Invite { target: jid });
    s.settle();
    ana.sync();
    more[4].sync();
    assert!(says(&ana, "el límite es 6"));
    assert!(more[4].game.local.social.party.invitation.is_none());
    assert!(ana.kicked().is_none() && beto.kicked().is_none() && caro.kicked().is_none());
}

#[test]
fn a_member_turning_criminal_leaves_the_party() {
    let s = server();
    let mut ana = s.bot("Ana");
    let mut beto = open(&s, "Beto");
    join(&s, &mut ana, &mut beto);
    beto.send(&C2S::Faction(FactionC2S::Leave { npc: None }));
    s.settle();
    beto.sync();
    ana.sync();
    assert!(says(&beto, "Ahora sos criminal, no podés estar en un grupo con ciudadanos."), "{:?}", beto.console().collect::<Vec<_>>());
    assert!(!ana.game.local.social.party.in_party());
    assert!(!beto.game.local.social.party.in_party());
}

#[test]
fn experience_is_shared_with_members_nearby() {
    // An official Gallina (npcs.dat 501: hostile, 10 HP, GiveEXP=5) of map 12, as in
    // p1_combat; a warrior kills it while a party mate stands next to him.
    let root = ao_formats::paths::public_recursos_dir().unwrap_or_else(recursos_dir);
    let map = ao_formats::AoMap::load(&root, 12).expect("map 12");
    let hen = map
        .npcs
        .iter()
        .filter(|n| n.npc == 501)
        .find(|h| map.npcs.iter().all(|o| (o.x, o.y) == (h.x, h.y) || tile_distance((o.x, o.y), (h.x, h.y)) > 12))
        .expect("a lone Gallina on map 12");
    let spot = Dir::ALL
        .iter()
        .map(|d| (hen.x + d.delta().0, hen.y + d.delta().1))
        .find(|&(x, y)| ao_rules::walk::standable(&map, x, y))
        .expect("free tile next to the hen");
    let at = (12, spot.0, spot.1);
    // Each hit gives CLng(damage * 5 / 10) and the party divides it, rounding: with the
    // official ExpMult=1 two members get 0 per hit (as in AO20). `ExpMult` 10 shows it.
    let s = TestServer::with(|st| {
        st.config.world.exp_mult = 10;
        st.recursos = root.clone();
        st.start = Some(at);
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = at;
    });
    let mut hunter = s.connect();
    assert!(hunter.create_account("cazador", "secreta1"));
    assert!(hunter.create_character("Cazador", 1, WARRIOR, 1, 1));
    hunter.play("Cazador");
    hunter.wait_until("equipo inicial", |b| b.game.local.inventory.iter().flatten().any(|i| i.equipped));
    let mut mate = open(&s, "Compa");
    assert!(tile_distance(hunter.pos(), mate.pos()) < 20);
    join(&s, &mut hunter, &mut mate);
    let mate_exp0 = mate.game.stats.exp;
    let mut killed = false;
    for _ in 0..400 {
        if says(&hunter, "Has matado a Gallina") {
            killed = true;
            break;
        }
        let me = hunter.pos();
        let next = hunter.creatures().find(|(_, e)| e.name == "Gallina" && (e.x - me.0).abs() + (e.y - me.1).abs() == 1).map(|(_, e)| (e.x, e.y));
        if let Some(t) = next {
            hunter.send(&C2S::Turn { dir: Dir::towards(t.0 - me.0, t.1 - me.1) });
            let seq = hunter.game.next_seq();
            hunter.send(&C2S::Melee { seq });
        }
        s.advance(ATTACK_INTERVAL_MS);
        hunter.sync();
        assert!(hunter.hp() > 0, "the warrior died");
    }
    assert!(killed, "the hen died");
    mate.sync();
    assert!(
        mate.game.stats.exp > mate_exp0 || mate.level() > 1,
        "the party mate got its share: {:?} / {:?}",
        mate.console().collect::<Vec<_>>(),
        hunter.console().collect::<Vec<_>>()
    );
    assert!(says(&mate, "El grupo ha ganado"));
    assert!(says(&hunter, "El grupo ha ganado"));
}
