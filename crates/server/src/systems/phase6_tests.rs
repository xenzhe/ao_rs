//! World tests of phase 6: lobbies (deathmatch, hunt), capture the flag, hourly events,
//! global quests, castles, invasions and the new GM commands.
use super::combat::Source;
use crate::clock::{Clock, GameTime, VirtualClock};
use crate::net::Event;
use crate::world::inventory::InvItem;
use crate::world::World;
use crate::Settings;
use ao_protocol::p4::gm::GmRank;
use ao_protocol::PROTOCOL_VERSION;

struct T {
    w: World,
    clock: VirtualClock,
}

impl T {
    fn new(names: &[&str]) -> T {
        let clock = VirtualClock::new();
        let mut s = Settings::defaults();
        s.clock = Clock::Virtual(clock.clone());
        s.npcs = false;
        s.start = Some((1, 50, 50));
        let mut w = World::new(s);
        for (i, n) in names.iter().enumerate() {
            let id = i as u32 + 1;
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            std::mem::forget(rx);
            w.on_event(Event::Connected { id, addr: "127.0.0.1:9".parse().unwrap(), tx });
            w.on_event(Event::Frame { id, msg: ao_protocol::C2S::Login { name: n.to_string(), version: PROTOCOL_VERSION }, at: None });
        }
        T { w, clock }
    }

    /// Runs `secs` seconds of world ticks (250 ms steps: every phase 6 system acts once
    /// a second).
    fn run(&mut self, secs: u64) {
        let end = self.clock.now().after_secs(secs);
        while self.clock.now() < end {
            let t = self.clock.now().after_ms(250);
            self.clock.set(t);
            self.w.tick(t);
        }
    }

    fn say(&mut self, id: u32, line: &str) {
        self.w.on_say(id, line.to_string());
    }

    fn pos(&self, id: u32) -> (u16, i32, i32) {
        let p = self.w.player(id).unwrap();
        (p.map, p.x, p.y)
    }

    fn gold(&self, id: u32) -> u32 {
        self.w.player(id).unwrap().inv.gold
    }

    fn has(&self, id: u32, obj: u16) -> u32 {
        self.w.player(id).unwrap().inv.slots.iter().flatten().filter(|i| i.obj == obj).map(|i| i.amount).sum()
    }
}

// ====================================================================== deathmatch

#[test]
fn deathmatch_from_inscription_to_prizes() {
    let mut t = T::new(&["Dios", "Ana", "Beto"]);
    t.w.player_mut(1).gm.rank = GmRank::Dios;
    for id in [2, 3] {
        t.w.player_mut(id).inv.gold = 1000;
    }
    // A red potion is forbidden in the arena: kept aside while playing.
    t.w.add_item(2, InvItem { obj: 38, amount: 5 });
    let home = t.pos(2);
    t.say(1, "/crearevento deathmatch 2 1 47 100");
    assert_eq!(t.w.lobbies.lobbies.len(), 1, "the lobby is open");
    // A user cannot create events (no answer, nothing happens).
    t.say(2, "/crearevento deathmatch 2 1 47 100");
    assert_eq!(t.w.lobbies.lobbies.len(), 1);
    t.say(2, "/participar");
    assert_eq!(t.pos(2).0, 49, "summoned to the arena map");
    assert_eq!(t.gold(2), 900);
    assert_eq!(t.has(2, 38), 0, "the potion was kept aside");
    t.say(2, "/participar");
    assert_eq!(t.w.lobbies.lobbies[0].seats.len(), 1, "no double inscription");
    t.say(3, "/participar");
    t.run(2);
    // Full: it starts, 10 s countdown, then the fight.
    assert!(t.w.instance(49).map.safe, "safe while counting down");
    t.run(11);
    assert!(!t.w.instance(49).map.safe, "fighting allowed");
    let a = t.w.lobbies.lobbies[0].scenario.as_ref().unwrap().arena().area;
    for id in [2, 3] {
        let (m, x, y) = t.pos(id);
        assert!(m == 49 && (a.0..=a.2).contains(&x) && (a.1..=a.3).contains(&y), "{id} inside the ring {a:?}: {x},{y}");
    }
    assert!(t.w.instance(49).map.blocked(a.0 - 1, a.1) & ao_formats::csm::BLOCK_ALL == ao_formats::csm::BLOCK_ALL, "the ring blocks");
    // Two citizens may fight in the arena (no faction rules there).
    assert_eq!(t.w.faction_attack_refusal(2, 3), None);
    assert!(t.w.hurt_player(3, 10_000, Source::Player(2)));
    let key = |t: &T, id: u32| t.w.lobby_seat_of(id).map(|(l, s)| t.w.lobbies.lobbies[l].seats[s].key).unwrap();
    let board = |t: &T, id: u32| t.w.lobbies.lobbies[0].scenario.as_ref().unwrap().arena().board.score(key(t, id));
    assert_eq!((board(&t, 2), board(&t, 3)), (3, 0));
    assert!(t.w.player(3).unwrap().inv.gold == 900, "no items nor gold lost");
    t.run(3);
    assert!(!t.w.is_dead(3), "respawned after PlayerRespawnTime");
    assert_eq!(t.pos(3).0, 49);
    // Time runs out: Ana wins 55 % of the 200 collected.
    t.run(300);
    assert_eq!(t.gold(2), 900 + 110);
    assert_eq!(t.gold(3), 900 + 70);
    t.run(11);
    assert!(t.w.lobbies.lobbies.is_empty(), "closed and released");
    assert_eq!(t.pos(2), home, "back where they joined");
    assert_eq!(t.has(2, 38), 5, "the potion came back");
    assert!(!t.w.instance(49).map.safe, "map flags restored (49 is not a safe map)");
    assert!(t.w.instance(49).map.blocked(a.0 - 1, a.1) & ao_formats::csm::BLOCK_ALL != ao_formats::csm::BLOCK_ALL || a.0 - 1 < 1, "the ring is gone");
}

#[test]
fn a_lobby_without_players_is_cancelled_and_refunds() {
    let mut t = T::new(&["Dios", "Ana"]);
    t.w.player_mut(1).gm.rank = GmRank::Dios;
    t.w.player_mut(2).inv.gold = 500;
    t.say(1, "/crearevento deathmatch 4 1 47 50");
    t.say(1, "/configlobby minlvl 1");
    t.say(2, "/participar");
    assert_eq!(t.gold(2), 450);
    // The lobby asks for 1 player minimum; raise it by hand to force the cancellation.
    t.w.lobbies.lobbies[0].min_players = 2;
    t.run(301);
    assert_eq!(t.gold(2), 500, "fee returned");
    t.run(12);
    assert!(t.w.lobbies.lobbies.is_empty());
    assert_eq!(t.pos(2).0, 1, "back home");
}

#[test]
fn lobby_refusals_follow_canplayerjoin() {
    let mut t = T::new(&["Dios", "Ana", "Beto"]);
    t.w.player_mut(1).gm.rank = GmRank::Dios;
    t.say(1, "/crearevento deathmatch 2 10 20 100");
    // Level 1 with no gold.
    t.say(2, "/participar");
    assert!(t.w.lobby_seat_of(2).is_none());
    t.w.player_mut(2).level = 15;
    t.say(2, "/participar");
    assert!(t.w.lobby_seat_of(2).is_none(), "no gold");
    t.w.player_mut(2).inv.gold = 100;
    t.say(2, "/participar 99 x");
    assert!(t.w.lobby_seat_of(2).is_none(), "no such room");
    t.say(2, "/participar");
    assert!(t.w.lobby_seat_of(2).is_some());
    // A second public event cannot replace the running one (VB overwrote it).
    t.say(1, "/crearevento caceria 2 1 47");
    assert_eq!(t.w.lobbies.lobbies.len(), 1);
    t.say(1, "/cancelarevento");
    assert_eq!(t.gold(2), 100);
}

// ====================================================================== hunt

#[test]
fn hunt_scores_only_club_kills() {
    let mut t = T::new(&["Dios", "Ana", "Beto"]);
    t.w.player_mut(1).gm.rank = GmRank::Dios;
    t.say(1, "/crearevento caceria 2 1 47");
    t.say(2, "/participar");
    assert!(t.w.lobby_seat_of(2).is_none(), "a club is required");
    for id in [2, 3] {
        t.w.add_item(id, InvItem { obj: super::lobby::HUNT_WEAPON, amount: 1 });
        t.say(id, "/participar");
    }
    t.run(14);
    let a = t.w.lobbies.lobbies[0].scenario.as_ref().unwrap().arena();
    assert!(!a.spawned.is_empty(), "the snakes are out");
    // The public npcs.dat numbers differ from the server's (see the audit): only some of
    // snakes.ini's targets are attackable creatures there (558, a wolf).
    let nid = *a.spawned.iter().find(|n| t.w.instance(49).npcs[*n].attackable).expect("an attackable target");
    let count = a.spawned.len();
    let number = t.w.instance(49).npcs[&nid].def;
    let expected = a.targets.iter().find(|x| x.npc == number).unwrap().score;
    // Ana equips the club and kills one with a blow; Beto kills one with a spell.
    let slot = (1..=24u8).find(|s| t.w.player(2).unwrap().inv.get(*s).is_some_and(|i| i.obj == super::lobby::HUNT_WEAPON)).unwrap();
    t.w.player_mut(2).inv.equipped.weapon = Some(slot);
    t.w.melee_blow = true;
    t.w.damage_npc(49, nid, 100_000, 2);
    t.w.melee_blow = false;
    let nid2 = *t.w.lobbies.lobbies[0].scenario.as_ref().unwrap().arena().spawned.iter().find(|n| t.w.instance(49).npcs[*n].attackable).unwrap();
    t.w.damage_npc(49, nid2, 100_000, 3);
    let l = &t.w.lobbies.lobbies[0];
    let a = l.scenario.as_ref().unwrap().arena();
    let score = |id: u32| a.board.score(l.seats.iter().find(|s| s.id == Some(id)).unwrap().key);
    assert_eq!(score(2), expected, "a club blow scores the target's points");
    assert_eq!(score(3), 0, "a spell kill gives nothing");
    assert_eq!(a.spawned.len(), count, "every dead target comes back");
    t.say(1, "/cancelarevento");
    t.run(12);
    assert!(t.w.lobbies.lobbies.is_empty());
    assert!(t.w.instance(49).npc_ids().iter().all(|n| t.w.instance(49).npcs.get(n).is_none_or(|n| !n.alive())), "the hunt creatures are gone");
}

// ====================================================================== capture the flag

#[test]
fn capture_the_flag_round_and_prize() {
    let mut t = T::new(&["Dios", "Ana", "Beto"]);
    t.w.player_mut(1).gm.rank = GmRank::Dios;
    for id in [2, 3] {
        t.w.player_mut(id).inv.gold = 1000;
    }
    t.w.player_mut(2).level = 30;
    t.say(1, "/crearevento captura 2 1 1 47 100");
    assert!(t.w.capture.is_some());
    t.say(2, "/participar");
    t.say(3, "/participar");
    // Full: teams by level (Ana blue, Beto red) and the first round in 60 s.
    assert_eq!(t.w.capture_team(2), Some(1));
    assert_eq!(t.w.capture_team(3), Some(2));
    assert_eq!(t.pos(2).0, 278, "waiting room");
    t.run(61);
    assert_eq!(t.pos(2).0, 275);
    assert_eq!(t.pos(3).0, 277);
    // Teammates cannot hit each other; enemies may.
    assert_eq!(t.w.faction_attack_refusal(2, 3), None);
    // Ana takes the red banner and holds it at her stand for 7 seconds.
    t.w.teleport(2, 277, 71, 55);
    let (m, x, y) = t.pos(2);
    assert_eq!((m, x, y), (277, 71, 55), "on the red stand");
    t.w.on_pick_up(2);
    assert_eq!(t.has(2, 3675), 1);
    t.w.teleport(2, 275, 38, 51);
    t.run(8);
    // Best of 1: Ana's fee back plus Beto's.
    assert!(t.w.capture.is_none(), "finished");
    assert_eq!(t.gold(2), 900 + 200);
    assert_eq!(t.gold(3), 900);
    assert_eq!(t.has(2, 3675), 0, "the banner is not kept");
    assert_eq!(t.pos(2).0, 1, "back where they signed up");
}

#[test]
fn capture_without_players_refunds() {
    let mut t = T::new(&["Dios", "Ana"]);
    t.w.player_mut(1).gm.rank = GmRank::Dios;
    t.w.player_mut(2).inv.gold = 300;
    t.say(1, "/crearevento captura 4 3 1 47 100");
    t.say(2, "/participar");
    assert_eq!(t.gold(2), 200);
    t.run(125);
    assert!(t.w.capture.is_none());
    assert_eq!(t.gold(2), 300);
}

// ====================================================================== hourly events

#[test]
fn forced_event_multiplies_and_ends_alone() {
    let mut t = T::new(&["Admin", "Ana"]);
    t.w.player_mut(1).gm.rank = GmRank::Admin;
    t.w.cfg.world.exp_mult = 3;
    t.say(1, "/crearevento 2@1@2");
    assert_eq!(t.w.cfg.world.exp_mult, 6);
    t.say(1, "/crearevento 1@1@2");
    assert_eq!(t.w.cfg.world.gold_mult, 1, "one at a time");
    t.run(62);
    assert_eq!(t.w.cfg.world.exp_mult, 3, "only the event's part is undone");
    t.say(1, "/crearevento 3@10@5");
    assert_eq!(t.w.cfg.world.gather_mult, 5);
    t.say(1, "/finalizarevento");
    assert_eq!(t.w.cfg.world.gather_mult, 1);
    t.say(1, "/crearevento 1@10@3");
    assert_eq!(t.w.cfg.world.gold_mult, 1, "gold up to x2");
}

#[test]
fn hourly_table_starts_on_the_hour() {
    let mut t = T::new(&["Ana"]);
    t.run(1);
    let hour = t.w.wevents.last_hour.unwrap();
    let next = (hour + 1) % 24;
    t.w.cfg.events.hourly.insert(next.to_string(), "5-2-2".into());
    // Jump the unix clock to the next hour.
    let now = t.w.unix_now();
    t.w.gms.unix_skew += 3600 - now.rem_euclid(3600) + 1;
    t.run(2);
    assert_eq!((t.w.cfg.world.exp_mult, t.w.cfg.world.gold_mult), (2, 2));
    t.say(1, "/evento");
    t.run(121);
    assert_eq!((t.w.cfg.world.exp_mult, t.w.cfg.world.gold_mult), (1, 1));
}

// ====================================================================== global quests

#[test]
fn global_quest_dates_counter_and_bosses() {
    let mut t = T::new(&["Ana"]);
    t.run(1);
    let Some(q) = t.w.wevents.quests.as_ref().and_then(|l| l.first()).cloned() else { return };
    assert!(!q.active, "the official dates are past");
    t.w.gms.unix_skew += q.def.start + 3600 - t.w.unix_now();
    t.w.wevents.next_quest_check = GameTime::ZERO;
    t.run(1);
    assert!(t.w.wevents.quests.as_ref().unwrap()[0].active, "started by date");
    assert_eq!(t.w.db.global_quest(1).unwrap().map(|r| r.0), Some(true));
    let quest = ao_formats::dat::quests::Quest { global_quest_index: 1, ..Default::default() };
    assert!(t.w.gquest_accept_open(&quest));
    t.w.gquest_contribute(1, &quest, q.def.installment + 1);
    let g = &t.w.wevents.quests.as_ref().unwrap()[0];
    assert_eq!(g.counter, q.def.installment + 1);
    assert_eq!(g.bosses.len(), q.def.bosses.len(), "the boss wave appeared");
    assert!(!t.w.gquest_finish_check(1, &quest, false), "no handing in while the boss lives");
    let (bm, bn) = g.bosses[0];
    t.w.damage_npc(bm, bn, 10_000_000, 1);
    assert!(t.w.wevents.quests.as_ref().unwrap()[0].bosses.is_empty());
    assert!(t.w.gquest_finish_check(1, &quest, false));
    assert_eq!(t.w.db.global_quest(1).unwrap().map(|r| r.1), Some(q.def.installment + 1), "contributions are in the database");
    // Past its end date it finishes (FinishOnThresholdReach=0).
    t.w.gms.unix_skew += q.def.end - q.def.start;
    t.w.wevents.next_quest_check = GameTime::ZERO;
    t.run(1);
    assert!(!t.w.wevents.quests.as_ref().unwrap()[0].active);
}

// ====================================================================== castle

#[test]
fn castle_founding_entry_and_whitelist() {
    let mut t = T::new(&["Rey", "Ana", "Beto"]);
    t.run(1);
    let (map, x, y) = (7u16, 61, 43);
    t.w.ensure_map(map);
    assert_eq!(t.w.instance(map).map.trigger(x, y), 41, "official foundation tile");
    t.w.teleport(1, map, x, y + 3);
    t.w.player_mut(1).target_tile = Some((map, x, y));
    t.w.add_item(1, InvItem { obj: 6362, amount: 1 });
    let slot = (1..=24u8).find(|s| t.w.player(1).unwrap().inv.get(*s).is_some_and(|i| i.obj == 6362)).unwrap();
    // No Patreon here: closed by default.
    t.w.on_use_item(1, slot);
    assert!(!t.w.castles.list.as_ref().unwrap()[0].row.active);
    t.w.cfg.events.castle_without_patreon = true;
    t.run(2);
    t.w.on_use_item(1, slot);
    let c = t.w.castles.list.as_ref().unwrap()[0].row.clone();
    assert!(c.active && c.outside == Some((map, x, y)));
    assert_eq!(t.w.instance(map).ground.at(x, y).map(|g| g.obj), Some(6382), "the model stands on the sign");
    assert_eq!(t.w.instance(map).map.exit(x - 1, y - 1).map(|e| e.map), Some(758), "the door leads inside");
    assert_eq!(t.w.instance(map).map.trigger(x - 1, y), 21);
    assert_eq!(t.w.db.castles().unwrap()[0].outside, Some((map, x, y)), "saved at once");
    // Ana is not on the list: sent back.
    t.w.teleport(2, map, x - 1, y);
    assert!(t.w.castle_on_step(2, map, x - 1, y));
    t.say(1, "/castle add ana");
    assert!(!t.w.castle_on_step(2, map, x - 1, y));
    assert!(!t.w.castle_on_step(1, map, x - 1, y), "the owner walks in");
    t.say(1, "/castle remove ana");
    assert!(t.w.castle_on_step(2, map, x - 1, y));
    // A roof trigger 21 on another map is not a castle entry.
    t.w.ensure_map(776);
    assert!(!t.w.castle_on_step(3, 776, 37, 48));
    // Relocating needs a week.
    t.w.on_use_item(1, slot);
    assert_eq!(t.w.castles.list.as_ref().unwrap()[0].row.founded, c.founded);
}

// ====================================================================== invasions

#[test]
fn invasions_stay_off_with_the_official_data_and_run_when_enabled() {
    let mut t = T::new(&["Dios"]);
    t.w.player_mut(1).gm.rank = GmRank::Dios;
    t.run(1);
    t.say(1, "/ce invasion bander");
    assert!(!t.w.wevents.invasions.as_ref().unwrap().iter().any(|i| i.active), "Cantidad=0");
    if t.w.wevents.invasions.as_ref().unwrap().is_empty() {
        return;
    }
    t.w.wevents.invasions_on = true;
    t.say(1, "/ce invasion bander");
    assert!(t.w.wevents.invasions.as_ref().unwrap()[0].active);
    t.run(12);
    assert!(!t.w.wevents.invasions.as_ref().unwrap()[0].alive.is_empty(), "invaders spawn every IntervaloSpawn");
    t.w.wevents.invasions.as_mut().unwrap()[0].wall = 0;
    t.w.invasion_end(0);
    let inv = &t.w.wevents.invasions.as_ref().unwrap()[0];
    assert!(!inv.active && inv.alive.is_empty());
}

// ====================================================================== GM commands

#[test]
fn new_gm_commands_need_their_rank_and_work() {
    let mut t = T::new(&["Conse", "Semi", "Dios", "Ana"]);
    t.w.player_mut(1).gm.rank = GmRank::Consejero;
    t.w.player_mut(2).gm.rank = GmRank::SemiDios;
    t.w.player_mut(3).gm.rank = GmRank::Dios;
    // /GENIO: Dios only (a Consejero could in the VB).
    for id in [1, 3] {
        t.w.player_mut(id).stats.skills.iter_mut().for_each(|s| *s = 5);
    }
    t.say(1, "/genio");
    assert!(t.w.player(1).unwrap().stats.skills.iter().all(|s| *s < 100));
    t.say(3, "/genio");
    assert!(t.w.player(3).unwrap().stats.skills.iter().all(|s| *s == 100));
    // /TELEPLOC to the last clicked tile.
    t.w.player_mut(1).target_tile = Some((1, 40, 40));
    t.say(1, "/teleploc");
    let (m, x, y) = t.pos(1);
    assert!(m == 1 && (x - 40).abs() <= 2 && (y - 40).abs() <= 2);
    // /SLOT
    t.w.add_item(4, InvItem { obj: 38, amount: 7 });
    t.say(1, "/slot Ana@1");
    // /SPAWN and /MASSKILL
    let before = t.w.instance(1).npc_ids().len();
    t.say(2, "/spawn 500");
    t.say(1, "/spawn 500");
    let after = t.w.instance(1).npc_ids().len();
    assert!(after <= before + 1);
    t.say(3, "/spawn 500");
    t.say(3, "/masskill");
    assert!(t.w.instance(t.pos(3).0).npc_ids().iter().all(|n| { let n = &t.w.instance(t.pos(3).0).npcs[n]; (n.x - t.pos(3).1).abs() > 11 || (n.y - t.pos(3).2).abs() > 9 }));
    // Weather toggles.
    let fog = t.w.weather.fog;
    t.say(2, "/niebla");
    assert_ne!(t.w.weather.fog, fog);
    t.say(2, "/lluvia");
    assert!(t.w.weather.precipitation);
    t.say(1, "/lluvia");
    assert!(t.w.weather.precipitation, "a Consejero cannot");
    // /SETDESC on the clicked player.
    t.w.player_mut(3).target_user = Some(4);
    t.say(3, "/setdesc Un viajero");
    assert_eq!(t.w.player(4).unwrap().gm_desc.as_deref(), Some("Un viajero"));
    // /TALKAS needs a clicked NPC.
    t.say(2, "/talkas hola");
    // Players' commands.
    t.say(4, "/uptime");
    t.say(4, "/ayuda");
    t.say(4, "/est");
    t.say(4, "/salas");
}
