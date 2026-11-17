//! Window: events: duels, tournaments, invasions, castles. OWNER: Opus D (docs/P4_CONTRATO.md).
//!
//! Three blocks, top to bottom: the duel (the one being fought, or the invitations, the
//! pending request and the challenge form), the GM tournament sign-up, and the invasion
//! status (off in the official data). `AO_EVENTDEMO=1` fills the window with sample data
//! (captures only; never sent to the server).
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_TEXT, GOOD, MUTED, TEXT};
use crate::ui::widgets::{button, measure, text, text_field, text_fit, text_with, tooltip};
use crate::ui::Ui;
use ao_protocol::p4::event::{DuelInvite, DuelView, EventView, TournamentSpec, TournamentView};
use macroquad::prelude::*;
use std::cell::RefCell;

#[derive(Default)]
struct Form {
    two_vs_two: bool,
    rival1: String,
    rival2: String,
    mate: String,
    bet: String,
    potions: String,
    refreshed: bool,
}

thread_local! {
    static FORM: RefCell<Form> = RefCell::new(Form { bet: "10000".into(), ..Default::default() });
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

fn header(label: &str, x: f32, y: f32, w: f32, u: f32) {
    text_with(FontKind::Title, label, x, y, 20.0 * u, GOLD_TEXT);
    let d = measure(label, 20.0 * u);
    draw_line(x + d.width + 10.0 * u, y - 6.0 * u, x + w, y - 6.0 * u, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.5));
}

fn row_bg(r: Rect) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, if r.contains(mouse_position().into()) { 0.40 } else { 0.25 }));
}

pub fn gold_str(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

fn vs(a: &[String], b: &[String]) -> String {
    format!("{}  vs  {}", a.join(" y "), b.join(" y "))
}

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = assets;
    let u = ui.u;
    if demo_active() {
        game.local.p4.event.view = demo_view(game);
    } else if !FORM.with(|f| std::mem::replace(&mut f.borrow_mut().refreshed, true)) {
        game.event_refresh();
    }
    let v = game.local.p4.event.view.clone();
    let mut y = r.y + 18.0 * u;

    header("Retos", r.x, y, r.w, u);
    y += 14.0 * u;
    if let Some(d) = &v.duel {
        y = duel_block(game, d, r, y, u);
    } else {
        for inv in &v.invites {
            let row = Rect::new(r.x, y, r.w, 46.0 * u);
            row_bg(row);
            text_fit(&format!("{} te desafía: {}", inv.offerer, vs(&inv.team_a, &inv.team_b)), row.x + 8.0 * u, row.y + 18.0 * u, 17.0 * u, row.w - 120.0 * u, TEXT);
            let pots = if inv.max_potions >= 0 { format!(" · máx. {} pociones rojas", inv.max_potions) } else { String::new() };
            text_fit(&format!("Apuesta {} de oro{pots}", gold_str(inv.bet)), row.x + 8.0 * u, row.y + 37.0 * u, 15.0 * u, row.w - 120.0 * u, GOLD_TEXT);
            if button(Rect::new(row.right() - 104.0 * u, row.y + 11.0 * u, 96.0 * u, 24.0 * u), "Aceptar", false, u) {
                let o = inv.offerer.clone();
                game.duel_accept(&o);
            }
            y += 50.0 * u;
        }
        if let Some(p) = &v.pending {
            let row = Rect::new(r.x, y, r.w, 46.0 * u);
            row_bg(row);
            let state = if p.queued { "Esperando una sala libre...".to_string() } else if p.missing.is_empty() { "Todos aceptaron".to_string() } else { format!("Falta aceptar: {}", p.missing.join(", ")) };
            text_fit(&format!("Tu reto: {}", vs(&p.team_a, &p.team_b)), row.x + 8.0 * u, row.y + 18.0 * u, 17.0 * u, row.w - 120.0 * u, TEXT);
            text_fit(&format!("{} · {} de oro", state, gold_str(p.bet)), row.x + 8.0 * u, row.y + 37.0 * u, 15.0 * u, row.w - 120.0 * u, MUTED);
            if button(Rect::new(row.right() - 104.0 * u, row.y + 11.0 * u, 96.0 * u, 24.0 * u), "Cancelar", false, u) {
                game.duel_cancel();
            }
            y += 50.0 * u;
        }
        y = propose_block(ui, game, &v, r, y + 4.0 * u);
    }

    y += 14.0 * u;
    header("Torneo", r.x, y, r.w, u);
    y += 14.0 * u;
    y = tournament_block(game, v.tournament.as_ref(), r, y, u);

    y += 14.0 * u;
    header("Invasiones", r.x, y, r.w, u);
    text_fit("Ninguna ciudad está bajo ataque.", r.x, y + 22.0 * u, 16.0 * u, r.w, MUTED);
}

fn duel_block(game: &mut Game, d: &DuelView, r: Rect, mut y: f32, u: f32) -> f32 {
    let card = Rect::new(r.x, y, r.w, 128.0 * u);
    draw_rectangle(card.x, card.y, card.w, card.h, Color::new(0.10, 0.03, 0.02, 0.55));
    draw_rectangle_lines(card.x, card.y, card.w, card.h, 1.0, Color::new(0.75, 0.30, 0.20, 0.9));
    let (mine, theirs, wm, wt) = if d.mine_a { (&d.team_a, &d.team_b, d.wins_a, d.wins_b) } else { (&d.team_b, &d.team_a, d.wins_b, d.wins_a) };
    let cx = card.center().x;
    let size = 22.0 * u;
    let left = mine.join(" y ");
    let lw = measure(&left, size).width.min(card.w / 2.0 - 60.0 * u);
    text_fit(&left, cx - 50.0 * u - lw, card.y + 30.0 * u, size, card.w / 2.0 - 60.0 * u, GOOD);
    text_fit(&theirs.join(" y "), cx + 50.0 * u, card.y + 30.0 * u, size, card.w / 2.0 - 60.0 * u, BAD);
    let score = format!("{wm} - {wt}");
    let sd = measure(&score, 30.0 * u);
    text_with(FontKind::Title, &score, cx - sd.width / 2.0, card.y + 34.0 * u, 30.0 * u, GOLD_TEXT);
    let status = if d.countdown > 0 { format!("Ronda {} · comienza en {}", d.round, d.countdown) } else { format!("Ronda {} · ¡a pelear!", d.round) };
    let sd = measure(&status, 18.0 * u);
    text(&status, cx - sd.width / 2.0, card.y + 62.0 * u, 18.0 * u, if d.countdown > 0 { GOLD_TEXT } else { TEXT });
    let info = format!("Al mejor de 3 · pozo {} de oro · quedan {}:{:02}", gold_str(d.bet * (d.team_a.len() + d.team_b.len()) as u32), d.time_left / 60, d.time_left % 60);
    let id = measure(&info, 15.0 * u);
    text(&info, cx - id.width / 2.0, card.y + 84.0 * u, 15.0 * u, MUTED);
    let quit = Rect::new(card.right() - 112.0 * u, card.bottom() - 32.0 * u, 104.0 * u, 24.0 * u);
    if button(quit, "Abandonar", false, u) {
        game.duel_quit();
    }
    if quit.contains(mouse_position().into()) {
        tooltip(&["Abandonar el reto".into(), "Pierdes la apuesta: el rival se lleva el pozo.".into()], u);
    }
    y += card.h + 4.0 * u;
    y
}

fn propose_block(ui: &mut Ui, game: &mut Game, v: &EventView, r: Rect, mut y: f32) -> f32 {
    let u = ui.u;
    let rules = format!("Apuesta mínima {} · el reino se queda el {} % · al mejor de 3", gold_str(v.min_bet.max(1)), v.tax_percent);
    text_fit(&rules, r.x, y + 12.0 * u, 15.0 * u, r.w, MUTED);
    y += 34.0 * u;
    let mut send = None;
    FORM.with(|f| {
        let f = &mut *f.borrow_mut();
        let bw = 64.0 * u;
        if button(Rect::new(r.x, y, bw, 24.0 * u), "1 vs 1", !f.two_vs_two, u) {
            f.two_vs_two = false;
        }
        if button(Rect::new(r.x + bw + 6.0 * u, y, bw, 24.0 * u), "2 vs 2", f.two_vs_two, u) {
            f.two_vs_two = true;
        }
        let name_ok = |c: char| c.is_alphanumeric() || c == ' ';
        let digits = |c: char| c.is_ascii_digit();
        let fx = r.x + 2.0 * bw + 20.0 * u;
        let fw = (r.right() - fx - 8.0 * u) / 2.0;
        let label = |s: &str, x: f32, y: f32| text(s, x, y - 4.0 * u, 14.0 * u, MUTED);
        label("Rival", fx, y);
        text_field(ui, "ev_rival1", &mut f.rival1, Rect::new(fx, y, fw, 24.0 * u), 16, false, name_ok);
        if f.two_vs_two {
            label("Rival 2", fx + fw + 8.0 * u, y);
            text_field(ui, "ev_rival2", &mut f.rival2, Rect::new(fx + fw + 8.0 * u, y, fw, 24.0 * u), 16, false, name_ok);
        }
        y += 40.0 * u;
        if f.two_vs_two {
            label("Tu compañero", fx, y);
            text_field(ui, "ev_mate", &mut f.mate, Rect::new(fx, y, fw, 24.0 * u), 16, false, name_ok);
        }
        label("Apuesta (oro)", r.x, y);
        text_field(ui, "ev_bet", &mut f.bet, Rect::new(r.x, y, 2.0 * bw + 6.0 * u, 24.0 * u), 9, false, digits);
        label("Pociones rojas máx.", fx + fw + 8.0 * u, y);
        text_field(ui, "ev_pots", &mut f.potions, Rect::new(fx + fw + 8.0 * u, y, fw * 0.5, 24.0 * u), 4, false, digits);
        let go = Rect::new(r.right() - 112.0 * u, y + 34.0 * u, 104.0 * u, 26.0 * u);
        if button(go, "Desafiar", false, u) {
            let rivals = if f.two_vs_two { vec![f.rival1.clone(), f.rival2.clone()] } else { vec![f.rival1.clone()] };
            let mates = if f.two_vs_two { vec![f.mate.clone()] } else { vec![] };
            let pots = f.potions.trim().parse::<i16>().unwrap_or(-1);
            send = Some((rivals, mates, f.bet.trim().parse::<u32>().unwrap_or(0), pots));
        }
        if go.contains(mouse_position().into()) {
            tooltip(&["Enviar el desafío".into(), "El oro se descuenta solo cuando empieza el reto.".into(), "Pociones vacío: sin límite.".into()], u);
        }
        text_fit(&format!("Tu oro: {}", gold_str(game.local.gold)), r.x, y + 52.0 * u, 16.0 * u, r.w - 130.0 * u, GOLD_TEXT);
        y += 64.0 * u;
    });
    if let Some((rivals, mates, bet, pots)) = send {
        game.duel_propose(&rivals, &mates, bet, pots);
    }
    y
}

fn tournament_block(game: &mut Game, t: Option<&TournamentView>, r: Rect, mut y: f32, u: f32) -> f32 {
    let Some(t) = t else {
        text_fit("No hay torneos abiertos. Los organizan los administradores.", r.x, y + 8.0 * u, 16.0 * u, r.w, MUTED);
        return y + 16.0 * u;
    };
    let s = &t.spec;
    text_fit(&s.name, r.x, y + 8.0 * u, 18.0 * u, r.w - 130.0 * u, TEXT);
    let line = format!("Niveles {}-{} · inscripción {} de oro · {}/{} anotados", s.min_level, s.max_level, gold_str(s.fee), t.signed.len(), s.slots);
    text_fit(&line, r.x, y + 28.0 * u, 15.0 * u, r.w - 130.0 * u, MUTED);
    if !s.rules.is_empty() {
        text_fit(&format!("Reglas: {}", s.rules), r.x, y + 46.0 * u, 15.0 * u, r.w - 130.0 * u, MUTED);
        y += 18.0 * u;
    }
    let b = Rect::new(r.right() - 112.0 * u, y - 6.0 * u, 104.0 * u, 26.0 * u);
    if t.started {
        text("En curso", b.x + 16.0 * u, b.y + 18.0 * u, 17.0 * u, GOLD_TEXT);
    } else if t.me_signed {
        if button(b, "Darme de baja", false, u) {
            game.tournament_leave();
        }
    } else if button(b, "Inscribirme", false, u) {
        game.tournament_join();
    }
    if b.contains(mouse_position().into()) && !t.signed.is_empty() {
        let mut lines = vec!["Anotados".to_string()];
        lines.extend(t.signed.iter().cloned());
        tooltip(&lines, u);
    }
    y += 34.0 * u;
    y
}
