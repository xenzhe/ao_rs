//! P2 chat channels: whisper, yell, party, guild, global. Bots on a virtual clock.
//! Party / guild hooks return "nobody" until their owners land, so here those channels are
//! proven to refuse non-members; the with-members path is covered by a unit test in
//! `systems/chat.rs`.
use ao_protocol::social::channel::ChannelKind;
use ao_protocol::{ChannelC2S, C2S};
use ao_rules::magic::{VIEW_RANGE_X, VIEW_RANGE_Y};
use ao_rules::Dir;
use ao_testkit::{Bot, TestServer};

/// Map 2 (not a safe zone) with harmless, static creatures.
fn field() -> TestServer {
    TestServer::with(|st| {
        st.start = Some((2, 50, 50));
        st.data.balance.creatures_per_map = 0;
        // Written against the public repo's map 2 (open field around 50,50).
        if let Some(p) = ao_formats::paths::public_recursos_dir() {
            st.recursos = p;
        }
    })
}

fn ch(bot: &mut Bot, msg: ChannelC2S) {
    bot.send(&C2S::Channel(msg));
}

/// `a` has sent something: wait for the server to consume it, then for `others` to receive
/// everything the server queued for them.
fn settle(s: &TestServer, bots: &mut [&mut Bot]) {
    s.settle();
    for b in bots.iter_mut() {
        b.sync();
    }
}

fn lines(bot: &Bot, kind: ChannelKind) -> Vec<(String, String, String)> {
    bot.game.local.social.channel.lines(kind).map(|l| (l.author.clone(), l.target.clone(), l.text.clone())).collect()
}

fn said(bot: &Bot, text: &str) -> bool {
    bot.console().any(|l| l.contains(text))
}

/// Walks `bot` (heading Right, Down or Up when blocked) until it is out of `other`'s view.
fn walk_away(s: &TestServer, bot: &mut Bot, other: (i32, i32)) {
    let far = |p: (i32, i32)| (p.0 - other.0).abs() > VIEW_RANGE_X || (p.1 - other.1).abs() > VIEW_RANGE_Y;
    let mut dir = Dir::Right;
    for _ in 0..400 {
        if far(bot.pos()) {
            bot.sync();
            return;
        }
        let before = bot.pos();
        if bot.walk(dir) {
            s.advance(300);
        } else {
            s.advance(100);
        }
        bot.pump();
        if bot.pos() == before {
            dir = match dir {
                Dir::Right => Dir::Down,
                Dir::Down => Dir::Up,
                _ => Dir::Right,
            };
        }
    }
    panic!("could not get out of view from {:?}", bot.pos());
}

#[test]
fn whisper_reaches_only_target_and_echoes_to_sender() {
    let s = field();
    let (mut ana, mut bea, mut cai) = (s.bot("Ana"), s.bot("Bea"), s.bot("Cai"));
    ch(&mut ana, ChannelC2S::Whisper { to: "bea".into(), text: "hola Bea".into() });
    settle(&s, &mut [&mut ana, &mut bea, &mut cai]);
    let want = vec![("Ana".to_string(), "Bea".to_string(), "hola Bea".to_string())];
    assert_eq!(lines(&ana, ChannelKind::Whisper), want, "echo to the sender");
    assert_eq!(lines(&bea, ChannelKind::Whisper), want, "target, name is case-insensitive");
    assert!(lines(&cai, ChannelKind::Whisper).is_empty(), "third party hears nothing");
    assert_eq!(bea.game.local.social.channel.last_whisper_from.as_deref(), Some("Ana"));
}

#[test]
fn whisper_traps_self_nobody_far_and_empty() {
    let s = field();
    let (mut ana, mut bea) = (s.bot("Ana"), s.bot("Bea"));
    ch(&mut ana, ChannelC2S::Whisper { to: "Ana".into(), text: "yo".into() });
    ch(&mut ana, ChannelC2S::Whisper { to: "Nadie".into(), text: "?".into() });
    ch(&mut ana, ChannelC2S::Whisper { to: "".into(), text: "?".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    assert!(said(&ana, "No puedes susurrarte a ti mismo"));
    assert_eq!(ana.console().filter(|l| l.contains("muy lejos o desconectado")).count(), 2, "nobody and empty name");
    assert!(lines(&ana, ChannelKind::Whisper).is_empty());

    // Empty / control-only text is dropped without a word.
    s.advance_secs(5);
    ana.game.console.clear();
    ch(&mut ana, ChannelC2S::Whisper { to: "Bea".into(), text: "   ".into() });
    ch(&mut ana, ChannelC2S::Whisper { to: "Bea".into(), text: "\u{7}\u{1b}".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    assert!(lines(&bea, ChannelKind::Whisper).is_empty());
    let extra: Vec<_> = ana.console().filter(|l| !l.contains("ataco")).collect(); // world noise
    assert!(extra.is_empty(), "silent drop, got {extra:?}");

    // Out of view: same text as "disconnected" (does not reveal who is online elsewhere).
    s.advance_secs(5);
    walk_away(&s, &mut bea, ana.pos());
    ch(&mut ana, ChannelC2S::Whisper { to: "Bea".into(), text: "lejos".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    assert!(lines(&bea, ChannelKind::Whisper).is_empty());
    assert!(said(&ana, "muy lejos o desconectado"));
}

#[test]
fn control_chars_are_stripped_and_long_lines_are_cut() {
    let s = field();
    let (mut ana, mut bea) = (s.bot("Ana"), s.bot("Bea"));
    // Under the 512-byte frame cap but over MAX_CHAT: the server cuts it.
    let long = format!("a\u{7}b{}", "x".repeat(400));
    ch(&mut ana, ChannelC2S::Whisper { to: "Bea".into(), text: long });
    settle(&s, &mut [&mut ana, &mut bea]);
    let got = lines(&bea, ChannelKind::Whisper);
    assert_eq!(got.len(), 1);
    assert!(got[0].2.starts_with("abxx") && got[0].2.chars().count() == ao_rules::MAX_CHAT && !got[0].2.chars().any(char::is_control));

    // A frame over the cap (what a cheat sends) costs the sender its connection, nobody else.
    ch(&mut ana, ChannelC2S::Global { text: "x".repeat(5000) });
    ana.wait_until("cierre por trama enorme", |b| b.closed());
    s.advance_secs(1);
    ch(&mut bea, ChannelC2S::Yell { text: "sigo aqui".into() });
    settle(&s, &mut [&mut bea]);
    assert_eq!(lines(&bea, ChannelKind::Yell).len(), 1, "the server keeps working");
}

#[test]
fn yell_reaches_the_visible_and_not_the_far() {
    let s = field();
    let (mut ana, mut bea, mut cai) = (s.bot("Ana"), s.bot("Bea"), s.bot("Cai"));
    walk_away(&s, &mut cai, ana.pos());
    s.advance_secs(1);
    ch(&mut ana, ChannelC2S::Yell { text: "¡hey!".into() });
    settle(&s, &mut [&mut ana, &mut bea, &mut cai]);
    let want = vec![("Ana".to_string(), String::new(), "¡hey!".to_string())];
    assert_eq!(lines(&ana, ChannelKind::Yell), want);
    assert_eq!(lines(&bea, ChannelKind::Yell), want);
    assert!(lines(&cai, ChannelKind::Yell).is_empty());
}

#[test]
fn party_and_guild_refuse_non_members() {
    let s = field();
    let (mut ana, mut bea) = (s.bot("Ana"), s.bot("Bea"));
    ch(&mut ana, ChannelC2S::Party { text: "grupo".into() });
    s.advance_secs(1);
    ch(&mut ana, ChannelC2S::Guild { text: "clan".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    assert!(said(&ana, "No estás en un grupo"));
    assert!(said(&ana, "No perteneces a ningún clan"));
    for b in [&ana, &bea] {
        assert!(lines(b, ChannelKind::Party).is_empty() && lines(b, ChannelKind::Guild).is_empty());
    }
}

#[test]
fn global_costs_gold_and_has_an_interval() {
    let s = field();
    let (mut ana, mut bea) = (s.bot("Ana"), s.bot("Bea"));
    // No gold: refused and nobody hears it.
    ch(&mut ana, ChannelC2S::Global { text: "hola mundo".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    assert!(said(&ana, "monedas de oro"));
    assert!(lines(&bea, ChannelKind::Global).is_empty());

    // Gold (obj 12) via the development command, then a line goes to everyone and is charged.
    ana.say("/dar 12 6000");
    settle(&s, &mut [&mut ana]);
    assert_eq!(ana.game.local.gold, 6000);
    s.advance_secs(6);
    ch(&mut ana, ChannelC2S::Global { text: "hola mundo".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    let want = vec![("Ana".to_string(), String::new(), "hola mundo".to_string())];
    assert_eq!(lines(&bea, ChannelKind::Global), want);
    assert_eq!(lines(&ana, ChannelKind::Global), want);
    assert_eq!(ana.game.local.gold, 6000 - 2500);

    // Too fast: refused, not charged.
    s.advance_secs(1);
    ch(&mut ana, ChannelC2S::Global { text: "otra".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    assert!(said(&ana, "tan rápido"));
    assert_eq!(ana.game.local.gold, 3500);
    assert_eq!(lines(&bea, ChannelKind::Global).len(), 1);

    // After the interval it works again.
    s.advance_secs(6);
    ch(&mut ana, ChannelC2S::Global { text: "otra".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    assert_eq!(lines(&bea, ChannelKind::Global).len(), 2);
    assert_eq!(ana.game.local.gold, 1000);
}

#[test]
fn flood_is_cut_but_recovers() {
    let s = field();
    let (mut ana, mut bea) = (s.bot("Ana"), s.bot("Bea"));
    for i in 0..10 {
        ch(&mut ana, ChannelC2S::Yell { text: format!("m{i}") });
    }
    settle(&s, &mut [&mut ana, &mut bea]);
    assert_eq!(lines(&bea, ChannelKind::Yell).len(), 4, "limiter: 4 lines per 3 s");
    assert!(said(&ana, "No hagas flood"));
    assert!(!ana.closed(), "flood is silenced, not kicked");
    s.advance_secs(4);
    ch(&mut ana, ChannelC2S::Yell { text: "otra vez".into() });
    settle(&s, &mut [&mut ana, &mut bea]);
    assert_eq!(lines(&bea, ChannelKind::Yell).len(), 5);
}
