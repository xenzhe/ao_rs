//! Window: the post office (TSAO `frmCorreo`). Inbox on the left (unread in bold wax, a
//! seal when a parcel waits); on the right the open letter, or the form to write one with
//! up to five stacks from the bag (select one in the inventory, then "Adjuntar") and gold.
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon, INK_DIM, WAX};
use crate::ui::inventory::selected_inv;
use crate::ui::theme::{FontKind, GOLD, INK};
use crate::ui::widgets::{button_kind, text_field, wrap_lines, Kind};
use crate::ui::{Ui, WindowId};
use ao_protocol::p4::mail::{BODY_MAX, PARCEL_MAX, SUBJECT_MAX};
use macroquad::prelude::*;
use std::cell::RefCell;

#[derive(Default)]
struct Form {
    writing: bool,
    to: String,
    subject: String,
    body: String,
    gold: String,
    /// `(slot, obj, amount)`.
    items: Vec<(u8, u16, u32)>,
}

thread_local! {
    static FORM: RefCell<Form> = RefCell::new(Form::default());
}

fn date(unix: i64) -> String {
    // Days since 1970 -> d/m/y (civil from days, Howard Hinnant).
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{d}/{m}/{y}")
}

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Mail);
    let Some((_, letters, postage, capacity)) = game.local.p4.mail.open.clone() else { return };
    if std::mem::take(&mut game.local.p4.mail.sent) {
        FORM.with(|f| *f.borrow_mut() = Form::default());
    }
    let left = Rect::new(r.x + 6.0 * u, r.y, (r.w * 0.40).floor(), r.h);
    let right = Rect::new(left.right() + 22.0 * u, r.y, r.right() - left.right() - 28.0 * u, r.h);
    draw_line(left.right() + 11.0 * u, r.y + 4.0 * u, left.right() + 11.0 * u, r.bottom() - 4.0 * u, 1.0, kit::alpha(GOLD, 0.35));

    // ---- inbox
    let mut y = kit::section(&format!("Buzón  {}/{}", letters.len(), capacity), left.x, left.y, left.w, u);
    let open_id = game.local.p4.mail.letter.as_ref().map(|l| l.id);
    let row_h = 48.0 * u;
    let max_rows = ((left.bottom() - y - 50.0 * u) / (row_h + 4.0 * u)).floor().max(1.0) as usize;
    if letters.is_empty() {
        kit::ink(FontKind::NarrativeItalic, "No tienes cartas.", left.x + 8.0 * u, y + 24.0 * u, 16.0 * u, INK_DIM);
    }
    for l in letters.iter().take(max_rows) {
        let row = Rect::new(left.x, y, left.w, row_h);
        let hover = row.contains(mouse_position().into());
        kit::page(row, u);
        if open_id == Some(l.id) || hover {
            draw_rectangle(row.x, row.y, row.w, row.h, Color::new(0.5, 0.09, 0.07, 0.08));
        }
        let kind = if l.unread { FontKind::Title } else { FontKind::Body };
        kit::ink_fit(kind, &l.subject, row.x + 12.0 * u, row.y + 21.0 * u, 16.0 * u, row.w - 50.0 * u, if l.unread { WAX } else { INK });
        kit::ink_fit(FontKind::Body, &format!("{} · {}", l.sender, date(l.sent_at)), row.x + 12.0 * u, row.y + 39.0 * u, 13.0 * u, row.w - 50.0 * u, INK_DIM);
        if l.parcel {
            kit::icon(Icon::Coin, row.right() - 22.0 * u, row.y + row_h / 2.0, 16.0 * u, Color::new(0.75, 0.52, 0.08, 1.0));
        }
        if hover && is_mouse_button_pressed(MouseButton::Left) {
            FORM.with(|f| f.borrow_mut().writing = false);
            game.mail_read(l.id);
        }
        y += row_h + 4.0 * u;
    }
    if button_kind(Rect::new(left.x, left.bottom() - 36.0 * u, left.w, 32.0 * u), "Escribir carta", Kind::Primary, true, u) {
        FORM.with(|f| f.borrow_mut().writing = true);
        game.local.p4.mail.letter = None;
    }

    // ---- right pane
    let writing = FORM.with(|f| f.borrow().writing);
    if writing {
        compose(ui, game, assets, right, postage);
    } else if let Some(l) = game.local.p4.mail.letter.clone() {
        let mut y = kit::section(&l.subject, right.x, right.y, right.w, u);
        kit::ink(FontKind::Body, &format!("De {} · {}", l.sender, date(l.sent_at)), right.x, y + 14.0 * u, 14.0 * u, INK_DIM);
        y += 30.0 * u;
        for line in wrap_lines(FontKind::Narrative, &l.body, 17.0 * u, right.w - 8.0 * u).iter().take(14) {
            kit::ink(FontKind::Narrative, line, right.x, y + 16.0 * u, 17.0 * u, INK);
            y += 22.0 * u;
        }
        let parcel = l.gold > 0 || !l.items.is_empty();
        let by = right.bottom() - 36.0 * u;
        if parcel {
            let py = by - 62.0 * u;
            kit::ink(FontKind::Title, "Paquete", right.x, py, 15.0 * u, WAX);
            let mut x = right.x;
            for (obj, amount) in &l.items {
                let cell = Rect::new(x, py + 8.0 * u, 40.0 * u, 40.0 * u);
                kit::page(cell, u);
                assets.draw_obj(*obj, cell);
                kit::ink(FontKind::Body, &amount.to_string(), cell.x + 3.0 * u, cell.bottom() - 4.0 * u, 12.0 * u, INK);
                x += 46.0 * u;
            }
            if l.gold > 0 {
                kit::icon(Icon::Coin, x + 12.0 * u, py + 28.0 * u, 16.0 * u, Color::new(0.75, 0.52, 0.08, 1.0));
                kit::ink(FontKind::Title, &kit::thousands(l.gold as u64), x + 26.0 * u, py + 34.0 * u, 16.0 * u, INK);
            }
            if button_kind(Rect::new(right.x, by, 160.0 * u, 32.0 * u), "Retirar paquete", Kind::Primary, true, u) {
                game.mail_take(l.id);
            }
        } else if button_kind(Rect::new(right.x, by, 120.0 * u, 32.0 * u), "Borrar", Kind::Danger, true, u) {
            game.mail_delete(l.id);
        }
    } else {
        kit::ink(FontKind::NarrativeItalic, "Elige una carta o escribe una nueva.", right.x + 8.0 * u, right.y + 40.0 * u, 16.0 * u, INK_DIM);
    }
}

fn compose(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect, postage: u32) {
    let u = ui.u;
    let mut y = kit::section("Nueva carta", r.x, r.y, r.w, u);
    let label = |s: &str, y: f32| kit::ink(FontKind::Body, s, r.x, y, 14.0 * u, INK_DIM);
    FORM.with(|f| {
        let mut f = f.borrow_mut();
        label("Para", y + 18.0 * u);
        text_field(ui, "mail_to", &mut f.to, Rect::new(r.x + 70.0 * u, y + 2.0 * u, r.w - 70.0 * u, 28.0 * u), 20, false, |c| c.is_alphanumeric() || c == '_' || c == ' ');
        y += 34.0 * u;
        label("Asunto", y + 18.0 * u);
        text_field(ui, "mail_subject", &mut f.subject, Rect::new(r.x + 70.0 * u, y + 2.0 * u, r.w - 70.0 * u, 28.0 * u), SUBJECT_MAX, false, |c| !c.is_control());
        y += 34.0 * u;
        label("Mensaje", y + 18.0 * u);
        text_field(ui, "mail_body", &mut f.body, Rect::new(r.x + 70.0 * u, y + 2.0 * u, r.w - 70.0 * u, 28.0 * u), BODY_MAX, false, |c| !c.is_control());
        y += 34.0 * u;
        label("Oro", y + 18.0 * u);
        text_field(ui, "mail_gold", &mut f.gold, Rect::new(r.x + 70.0 * u, y + 2.0 * u, 120.0 * u, 28.0 * u), 9, false, |c| c.is_ascii_digit());
        y += 44.0 * u;

        // Parcel: up to five stacks picked in the inventory window.
        kit::ink(FontKind::Title, &format!("Adjuntos {}/{}", f.items.len(), PARCEL_MAX), r.x, y, 15.0 * u, WAX);
        y += 8.0 * u;
        let mut x = r.x;
        let mut remove = None;
        for (i, (_, obj, amount)) in f.items.iter().enumerate() {
            let cell = Rect::new(x, y, 40.0 * u, 40.0 * u);
            kit::page(cell, u);
            assets.draw_obj(*obj, cell);
            kit::ink(FontKind::Body, &amount.to_string(), cell.x + 3.0 * u, cell.bottom() - 4.0 * u, 12.0 * u, INK);
            if cell.contains(mouse_position().into()) && is_mouse_button_pressed(MouseButton::Right) {
                remove = Some(i);
            }
            x += 46.0 * u;
        }
        if let Some(i) = remove {
            f.items.remove(i);
        }
        y += 48.0 * u;
        let sel = selected_inv();
        let pick = (sel > 0).then(|| game.local.inventory.get(sel as usize - 1).copied().flatten()).flatten();
        let can_add = f.items.len() < PARCEL_MAX && pick.is_some_and(|it| !it.equipped) && !f.items.iter().any(|(s, ..)| *s == sel);
        if button_kind(Rect::new(r.x, y, 200.0 * u, 30.0 * u), "Adjuntar seleccionado", Kind::Secondary, can_add, u) {
            if let Some(it) = pick {
                f.items.push((sel, it.obj, it.amount));
            }
        }
        kit::ink(FontKind::Body, "Clic derecho quita un adjunto", r.x + 210.0 * u, y + 20.0 * u, 13.0 * u, INK_DIM);

        let gold: u32 = f.gold.parse().unwrap_or(0);
        let by = r.bottom() - 36.0 * u;
        kit::ink(FontKind::Body, &format!("Envío: {} de oro", kit::thousands(postage as u64)), r.x, by - 10.0 * u, 14.0 * u, INK_DIM);
        let ok = !f.to.trim().is_empty() && !f.subject.trim().is_empty() && game.local.gold as u64 >= gold as u64 + postage as u64;
        if button_kind(Rect::new(r.right() - 140.0 * u, by, 140.0 * u, 32.0 * u), "Enviar", Kind::Primary, ok, u) {
            let items = f.items.iter().map(|(s, _, a)| (*s, *a)).collect();
            let (to, subject, body) = (f.to.clone(), f.subject.clone(), f.body.clone());
            game.mail_send(&to, &subject, &body, gold, items);
        }
    });
}
