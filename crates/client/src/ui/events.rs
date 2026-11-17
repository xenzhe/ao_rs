//! Window: events, laid out as a notice board. On the left the duels (the one being fought as
//! a versus poster, or the challenges you received, your pending one and the form to
//! challenge someone); on the right the tournament as a parchment poster with who has signed
//! up, and the invasion status (off in the official data). `AO_EVENTDEMO=1` fills the window
//! with sample data (captures only; never sent to the server).
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon, INK_DIM, WAX};
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, INK, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{button_kind, text_field, text_with, tooltip, Kind};
use crate::ui::{Ui, WindowId};
use ao_protocol::p4::event::{DuelInvite, DuelView, EventView, TournamentSpec, TournamentView};
use macroquad::prelude::*;
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct Form {
    two_vs_two: bool,
    rival1: String,
    rival2: String,
    mate: String,
    bet: String,
    potions: String,
}

thread_local! {
    static FORM: RefCell<Form> = RefCell::new(Form { bet: "10000".into(), ..Default::default() });
    /// Last frame the window was drawn and when the shown `State` arrived.
    static LAST_DRAW: Cell<f64> = const { Cell::new(-10.0) };
    static VIEW_AT: Cell<f64> = const { Cell::new(0.0) };
}

/// Shared by both event windows: asks for fresh state when a window (re)opens and returns
/// the seconds elapsed since the data it shows arrived.
pub(crate) fn sync_window(game: &mut Game, last_draw: &'static std::thread::LocalKey<Cell<f64>>, stamp: &'static std::thread::LocalKey<Cell<f64>>, fresh: bool) -> u32 {
    let now = get_time();
    if now - last_draw.get() > 0.5 && !demo_active() {
        game.event_refresh();
    }
    last_draw.set(now);
    if demo_active() {
        return 0;
    }
    if fresh {
        stamp.set(now);
    }
    (now - stamp.get()).max(0.0) as u32
}

/// `AO_EVENTDEMO=1`: sample state for screenshots (re-applied every frame).
pub fn demo_active() -> bool {
    std::env::var("AO_EVENTDEMO").is_ok_and(|v| !v.is_empty() && v != "0")
}

fn demo_view(game: &Game) -> EventView {
    let me = game.me().map(|e| e.name.clone()).unwrap_or_else(|| "Tester".into());
    let fighting = std::env::var("AO_EVENTDEMO").is_ok_and(|v| v == "2");
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    EventView {
        invites: if fighting { vec![] } else { vec![DuelInvite { offerer: "Morgana".into(), team_a: s(&["Morgana"]), team_b: vec![me.clone()], bet: 25_000, max_potions: 30, missing: vec![me.clone()], queued: false }] },
        pending: if fighting { None } else { Some(DuelInvite { offerer: me.clone(), team_a: vec![me.clone(), "Aldric".into()], team_b: s(&["Valen", "Tordek"]), bet: 10_000, max_potions: -1, missing: s(&["Tordek"]), queued: false }) },
        duel: fighting.then(|| DuelView { team_a: vec![me.clone()], team_b: s(&["Morgana"]), round: 2, wins_a: 1, wins_b: 0, mine_a: true, bet: 25_000, countdown: 7, time_left: 512 }),
        tournament: Some(TournamentView {
            spec: TournamentSpec { name: "Copa de Ullathorpe".into(), rules: "1 vs 1, sin pociones".into(), min_level: 15, max_level: 40, slots: 16, fee: 5_000, classes: vec![], map: 324, x: 50, y: 50 },
            signed: s(&["Aldric", "Valen", "Morgana", "Kira", "Brom"]),
            me_signed: false,
            started: false,
        }),
        min_bet: 10_000,
        max_team: 5,
        tax_percent: 10,
    }
}

pub fn gold_str(n: u32) -> String {
    kit::thousands(n as u64)
}

fn vs(a: &[String], b: &[String]) -> String {
    format!("{}  contra  {}", a.join(" y "), b.join(" y "))
}

/// A coin and an amount, in gold.
fn coin(x: f32, y: f32, amount: u32, size: f32, c: Color) -> f32 {
    kit::icon(Icon::Coin, x + size * 0.5, y - size * 0.3, size * 1.05, Color::new(0.95, 0.76, 0.26, 1.0));
    let s = gold_str(amount);
    text_with(FontKind::Title, &s, x + size * 1.2, y, size, c);
    size * 1.2 + crate::ui::widgets::measure_with(FontKind::Title, &s, size).width
}

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = assets;
    let u = ui.u;
    kit::begin(ui, WindowId::Events);
    if demo_active() {
        game.local.p4.event.view = demo_view(game);
    }
    let fresh = std::mem::take(&mut game.local.p4.event.view_new);
    let elapsed = sync_window(game, &LAST_DRAW, &VIEW_AT, fresh);
    let mut v = game.local.p4.event.view.clone();
    if let Some(d) = v.duel.as_mut() {
        d.countdown = d.countdown.saturating_sub(elapsed.min(u16::MAX as u32) as u16);
        d.time_left = d.time_left.saturating_sub(elapsed);
    }
    let r = Rect::new(r.x + 6.0 * u, r.y + 2.0 * u, r.w - 12.0 * u, r.h - 4.0 * u);
    let left = Rect::new(r.x, r.y, (r.w * 0.52).floor(), r.h);
    let right = Rect::new(left.right() + 24.0 * u, r.y, r.right() - left.right() - 24.0 * u, r.h);
    draw_line(left.right() + 12.0 * u, r.y + 4.0 * u, left.right() + 12.0 * u, r.bottom() - 4.0 * u, 1.0, kit::alpha(GOLD, 0.35));

    // ---- duels
    let mut y = kit::section("Retos", left.x, left.y, left.w, u);
    if let Some(d) = &v.duel {
        duel_poster(game, d, Rect::new(left.x, y, left.w, 260.0 * u), u);
    } else {
        for inv in &v.invites {
            let card = Rect::new(left.x, y, left.w, 78.0 * u);
            kit::page(card, u);
            kit::ink_fit(FontKind::Title, &format!("{} te desafía", inv.offerer), card.x + 16.0 * u, card.y + 28.0 * u, 17.0 * u, card.w - 140.0 * u, WAX);
            kit::ink_fit(FontKind::Narrative, &vs(&inv.team_a, &inv.team_b), card.x + 16.0 * u, card.y + 48.0 * u, 16.0 * u, card.w - 140.0 * u, INK);
            let pots = if inv.max_potions >= 0 { format!("máx. {} pociones rojas", inv.max_potions) } else { "sin límite de pociones".into() };
            kit::icon(Icon::Coin, card.x + 24.0 * u, card.y + 64.0 * u, 14.0 * u, Color::new(0.75, 0.52, 0.08, 1.0));
            kit::ink(FontKind::Body, &format!("{} de oro · {pots}", gold_str(inv.bet)), card.x + 38.0 * u, card.y + 69.0 * u, 14.0 * u, INK_DIM);
            if button_kind(Rect::new(card.right() - 110.0 * u, card.y + 22.0 * u, 96.0 * u, 32.0 * u), "Aceptar", Kind::Primary, true, u) {
                let o = inv.offerer.clone();
                game.duel_accept(&o);
            }
            y += 88.0 * u;
        }
        if let Some(p) = &v.pending {
            let card = Rect::new(left.x, y, left.w, 62.0 * u);
            let state = if p.queued { "Esperando una sala libre...".to_string() } else if p.missing.is_empty() { "Todos aceptaron".to_string() } else { format!("Falta aceptar: {}", p.missing.join(", ")) };
            draw_rectangle(card.x, card.y, card.w, card.h, Color::new(1.0, 0.8, 0.4, 0.05));
            draw_rectangle(card.x, card.y, 3.0 * u, card.h, GOLD_DIM);
            kit::fit(FontKind::Title, "Tu reto", card.x + 14.0 * u, card.y + 20.0 * u, 15.0 * u, card.w - 130.0 * u, GOLD_TEXT);
            kit::fit(FontKind::Body, &vs(&p.team_a, &p.team_b), card.x + 14.0 * u, card.y + 38.0 * u, 15.0 * u, card.w - 130.0 * u, TEXT);
            kit::fit(FontKind::NarrativeItalic, &format!("{} · {} de oro", state, gold_str(p.bet)), card.x + 14.0 * u, card.y + 55.0 * u, 14.0 * u, card.w - 130.0 * u, TEXT_DIM);
            if button_kind(Rect::new(card.right() - 100.0 * u, card.y + 16.0 * u, 96.0 * u, 28.0 * u), "Cancelar", Kind::Secondary, true, u) {
                game.duel_cancel();
            }
            y += 70.0 * u;
        }
        propose_block(ui, game, &v, left, y + 4.0 * u);
    }

    // ---- tournament poster
    let ty = kit::section("Torneo", right.x, right.y, right.w, u);
    let poster_h = 292.0 * u;
    tournament_poster(game, v.tournament.as_ref(), Rect::new(right.x, ty, right.w, poster_h), u);

    // ---- invasions
    let iy = kit::section("Invasiones", right.x, ty + poster_h + 14.0 * u, right.w, u);
    kit::icon(Icon::Shield, right.x + 12.0 * u, iy + 16.0 * u, 20.0 * u, Color::new(0.35, 0.62, 0.38, 1.0));
    text_with(FontKind::NarrativeItalic, "Ninguna ciudad está bajo ataque.", right.x + 32.0 * u, iy + 22.0 * u, 16.0 * u, TEXT_DIM);
}

/// The fight in progress as a versus poster: teams, score, round, pot and time.
fn duel_poster(game: &mut Game, d: &DuelView, r: Rect, u: f32) {
    // Dark red stage with a crossed-swords crest.
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.13, 0.04, 0.03, 0.9));
    draw_rectangle(r.x, r.y, r.w, 3.0 * u, Color::new(0.75, 0.25, 0.18, 0.9));
    draw_rectangle(r.x, r.bottom() - 3.0 * u, r.w, 3.0 * u, Color::new(0.75, 0.25, 0.18, 0.9));
    kit::glow(r.center().x, r.y + 70.0 * u, 80.0 * u, Color::new(1.0, 0.3, 0.2, 1.0));
    let (mine, theirs, wm, wt) = if d.mine_a { (&d.team_a, &d.team_b, d.wins_a, d.wins_b) } else { (&d.team_b, &d.team_a, d.wins_b, d.wins_a) };
    let cx = r.center().x;
    kit::text_mid(FontKind::Title, "TÚ", cx - r.w * 0.27, r.y + 28.0 * u, 13.0 * u, GOOD);
    kit::text_mid(FontKind::Title, "RIVAL", cx + r.w * 0.27, r.y + 28.0 * u, 13.0 * u, BAD);
    kit::fit_centered(FontKind::Title, &mine.join(" y "), cx - r.w * 0.27, r.y + 58.0 * u, 22.0 * u, r.w * 0.38, GOOD);
    kit::fit_centered(FontKind::Title, &theirs.join(" y "), cx + r.w * 0.27, r.y + 58.0 * u, 22.0 * u, r.w * 0.38, BAD);
    kit::text_mid(FontKind::Body, &format!("{wm}  -  {wt}"), cx, r.y + 108.0 * u, 54.0 * u, GOLD_BRIGHT);
    let status = if d.countdown > 0 { format!("Ronda {} · empieza en {}", d.round, d.countdown) } else { format!("Ronda {} · ¡a pelear!", d.round) };
    kit::text_mid(FontKind::Title, &status, cx, r.y + 146.0 * u, 18.0 * u, if d.countdown > 0 { GOLD_TEXT } else { TEXT });
    // Pot and time.
    let pot = d.bet * (d.team_a.len() + d.team_b.len()) as u32;
    let w = coin(cx - 130.0 * u, r.y + 184.0 * u, pot, 20.0 * u, GOLD_BRIGHT);
    let _ = w;
    kit::icon(Icon::Hourglass, cx + 90.0 * u, r.y + 178.0 * u, 18.0 * u, TEXT_DIM);
    text_with(FontKind::Title, &format!("{}:{:02}", d.time_left / 60, d.time_left % 60), cx + 104.0 * u, r.y + 184.0 * u, 20.0 * u, TEXT);
    text_with(FontKind::NarrativeItalic, "Al mejor de tres rondas", cx - 82.0 * u, r.y + 210.0 * u, 15.0 * u, TEXT_DIM);
    let quit = Rect::new(r.right() - 122.0 * u, r.bottom() - 40.0 * u, 112.0 * u, 30.0 * u);
    if button_kind(quit, "Abandonar", Kind::Danger, true, u) {
        game.duel_quit();
    }
    if kit::hover(quit) {
        tooltip(&["Abandonar el reto".into(), "~Pierdes la apuesta: el rival se lleva el pozo.".into()], u);
    }
}

fn propose_block(ui: &mut Ui, game: &mut Game, v: &EventView, r: Rect, y: f32) {
    let u = ui.u;
    let hy = kit::section("Lanzar un reto", r.x, y, r.w, u);
    let rules = format!("Apuesta mínima {} · el reino se queda el {} % · al mejor de 3", gold_str(v.min_bet.max(1)), v.tax_percent);
    text_with(FontKind::NarrativeItalic, &rules, r.x, hy + 12.0 * u, 14.0 * u, TEXT_DIM);
    let mut send = None;
    FORM.with(|f| {
        let f = &mut *f.borrow_mut();
        let mut y = hy + 22.0 * u;
        let labels = ["1 contra 1".to_string(), "2 contra 2".to_string()];
        if let Some(i) = kit::tabs(r.x + 4.0 * u, y, &labels, f.two_vs_two as usize, u) {
            f.two_vs_two = i == 1;
        }
        y += 38.0 * u;
        let name_ok = |c: char| c.is_alphanumeric() || c == ' ';
        let digits = |c: char| c.is_ascii_digit();
        let half = (r.w - 10.0 * u) / 2.0;
        let label = |s: &str, x: f32, y: f32| text_with(FontKind::Body, s, x, y, 13.0 * u, TEXT_DIM);
        label("Rival", r.x, y);
        text_field(ui, "ev_rival1", &mut f.rival1, Rect::new(r.x, y + 4.0 * u, if f.two_vs_two { half } else { r.w }, 26.0 * u), 16, false, name_ok);
        if f.two_vs_two {
            label("Segundo rival", r.x + half + 10.0 * u, y);
            text_field(ui, "ev_rival2", &mut f.rival2, Rect::new(r.x + half + 10.0 * u, y + 4.0 * u, half, 26.0 * u), 16, false, name_ok);
            y += 46.0 * u;
            label("Tu compañero", r.x, y);
            text_field(ui, "ev_mate", &mut f.mate, Rect::new(r.x, y + 4.0 * u, r.w, 26.0 * u), 16, false, name_ok);
        }
        y += 46.0 * u;
        label("Apuesta (oro)", r.x, y);
        text_field(ui, "ev_bet", &mut f.bet, Rect::new(r.x, y + 4.0 * u, half, 26.0 * u), 9, false, digits);
        label("Pociones rojas máx.", r.x + half + 10.0 * u, y);
        text_field(ui, "ev_pots", &mut f.potions, Rect::new(r.x + half + 10.0 * u, y + 4.0 * u, half, 26.0 * u), 4, false, digits);
        let go = Rect::new(r.right() - 150.0 * u, r.bottom() - 40.0 * u, 150.0 * u, 36.0 * u);
        if button_kind(go, "Desafiar", Kind::Primary, true, u) {
            let rivals = if f.two_vs_two { vec![f.rival1.clone(), f.rival2.clone()] } else { vec![f.rival1.clone()] };
            let mates = if f.two_vs_two { vec![f.mate.clone()] } else { vec![] };
            let pots = f.potions.trim().parse::<i16>().unwrap_or(-1);
            send = Some((rivals, mates, f.bet.trim().parse::<u32>().unwrap_or(0), pots));
        }
        if kit::hover(go) {
            tooltip(&["Enviar el desafío".into(), "~El oro se descuenta solo cuando empieza el reto.".into(), "Pociones vacío: sin límite.".into()], u);
        }
        kit::icon(Icon::Coin, r.x + 10.0 * u, r.bottom() - 20.0 * u, 18.0 * u, Color::new(0.95, 0.76, 0.26, 1.0));
        text_with(FontKind::Title, &gold_str(game.local.gold), r.x + 26.0 * u, r.bottom() - 14.0 * u, 17.0 * u, GOLD_TEXT);
    });
    if let Some((rivals, mates, bet, pots)) = send {
        game.duel_propose(&rivals, &mates, bet, pots);
    }
}

/// The tournament as a parchment poster: title, conditions, who signed up, and the button.
fn tournament_poster(game: &mut Game, t: Option<&TournamentView>, r: Rect, u: f32) {
    let Some(t) = t else {
        kit::empty(r, "No hay torneos abiertos.", "Los organizan los administradores.", u);
        return;
    };
    kit::page(r, u);
    let s = &t.spec;
    let px = r.x + 20.0 * u;
    let pw = r.w - 40.0 * u;
    kit::ink_mid(FontKind::Title, "TORNEO", r.center().x, r.y + 28.0 * u, 12.0 * u, pw, INK_DIM);
    kit::ink_mid(FontKind::Title, &s.name, r.center().x, r.y + 56.0 * u, 24.0 * u, pw, Color::new(0.32, 0.06, 0.04, 1.0));
    kit::rule(px, r.y + 66.0 * u, pw, Color::new(0.35, 0.2, 0.08, 0.9));
    if !s.rules.is_empty() {
        kit::ink_mid(FontKind::NarrativeItalic, &s.rules, r.center().x, r.y + 88.0 * u, 16.0 * u, pw, INK);
    }
    // Conditions: levels and fee.
    let cy = r.y + 118.0 * u;
    kit::icon(Icon::Star, px + 10.0 * u, cy, 16.0 * u, Color::new(0.55, 0.15, 0.1, 1.0));
    kit::ink(FontKind::Body, &format!("Niveles {} a {}", s.min_level, s.max_level), px + 26.0 * u, cy + 5.0 * u, 15.0 * u, INK);
    kit::icon(Icon::Coin, px + pw * 0.55 + 10.0 * u, cy, 16.0 * u, Color::new(0.75, 0.52, 0.08, 1.0));
    kit::ink(FontKind::Body, &format!("{} de inscripción", gold_str(s.fee)), px + pw * 0.55 + 26.0 * u, cy + 5.0 * u, 15.0 * u, INK);
    // Sign-up bar.
    let by = r.y + 140.0 * u;
    let frac = t.signed.len() as f32 / s.slots.max(1) as f32;
    draw_rectangle(px, by, pw, 9.0 * u, Color::new(0.35, 0.22, 0.1, 0.25));
    draw_rectangle(px, by, pw * frac.clamp(0.0, 1.0), 9.0 * u, Color::new(0.55, 0.15, 0.1, 0.95));
    kit::ink(FontKind::Body, &format!("{} de {} plazas", t.signed.len(), s.slots), px, by + 28.0 * u, 14.0 * u, INK_DIM);
    // The names signed up, as little tags.
    let mut tx = px;
    let mut ty = by + 42.0 * u;
    for n in &t.signed {
        let w = crate::ui::widgets::measure_with(FontKind::Body, n, 14.0 * u).width + 16.0 * u;
        if tx + w > px + pw {
            tx = px;
            ty += 24.0 * u;
        }
        if ty + 22.0 * u > r.bottom() - 54.0 * u {
            break;
        }
        draw_rectangle(tx, ty, w, 20.0 * u, Color::new(0.35, 0.2, 0.08, 0.14));
        draw_rectangle_lines(tx, ty, w, 20.0 * u, 1.0, Color::new(0.35, 0.2, 0.08, 0.5));
        kit::ink(FontKind::Body, n, tx + 8.0 * u, ty + 15.0 * u, 14.0 * u, INK);
        tx += w + 6.0 * u;
    }
    let b = Rect::new(r.center().x - 80.0 * u, r.bottom() - 48.0 * u, 160.0 * u, 34.0 * u);
    if t.started {
        kit::ink_mid(FontKind::Title, "EN CURSO", r.center().x, b.y + 24.0 * u, 18.0 * u, 160.0 * u, WAX);
    } else if t.me_signed {
        if button_kind(b, "Darme de baja", Kind::Secondary, true, u) {
            game.tournament_leave();
        }
    } else if button_kind(b, "Inscribirme", Kind::Primary, true, u) {
        game.tournament_join();
    }
    let _ = (GOLD_BRIGHT, TEXT, TEXT_FAINT);
}
