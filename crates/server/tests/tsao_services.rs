//! TSAO services over the wire with the converted Tierras Sagradas data (skipped without
//! X:\dev\tsao_rec): mail with a parcel between two account characters (paid postage, all
//! or nothing, taken once, forged slots), the clan vault for clanless players, and the
//! surgeon (paid, only faces of the race, only near the NPC).
use ao_protocol::p4::mail::MailC2S;
use ao_protocol::p4::surgeon::SurgeonC2S;
use ao_protocol::C2S;
use ao_testkit::{Bot, TestServer};
use std::path::PathBuf;

const GOLD_OBJ: u16 = 12;
/// Tanaris (map 28).
const CORREO: (i32, i32) = (64, 48);
const BOVEDA: (i32, i32) = (61, 51);
/// Thir (map 25).
const CIRUJANO: (i32, i32) = (40, 56);

fn tsao() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var("AO_TSAO").unwrap_or_else(|_| "X:/dev/tsao_rec".into()));
    p.join("Mapas").join("mapa28.csm").exists().then_some(p)
}

fn character(s: &TestServer, user: &str, name: &str) -> Bot {
    let mut b = s.connect();
    assert!(b.create_account(user, "clave-segura-1"));
    assert!(b.create_character(name, 1, 3, 1, 5));
    b.play(name);
    b
}

fn step(s: &TestServer, b: &mut Bot) {
    s.advance(50);
    b.sync();
}

fn says(b: &Bot, t: &str) -> bool {
    b.console().any(|l| l.contains(t))
}

#[test]
fn mail_vault_and_surgeon() {
    let Some(rec) = tsao() else { return eprintln!("TSAO data not found, skipping") };
    let s = TestServer::with(|st| {
        st.recursos = rec;
        st.start = Some((28, CORREO.0, CORREO.1 + 2));
        // Account characters start where balance.toml says.
        (st.data.balance.start_map, st.data.balance.start_x, st.data.balance.start_y) = (28, CORREO.0, CORREO.1 + 2);
    });
    let mut a = character(&s, "cuenta_a", "Remitente");
    let mut b = character(&s, "cuenta_b", "Destino");
    a.say(&format!("/dar {GOLD_OBJ} 5000"));
    a.say("/dar 38 7");
    step(&s, &mut a);
    let slot = a.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == 38)).expect("object given") as u8 + 1;
    let gold0 = a.game.local.gold;

    // Open the post office.
    a.send(&C2S::Interact { tx: CORREO.0, ty: CORREO.1 });
    a.wait_until("buzon", |b| b.game.local.p4.mail.open.is_some());
    let npc = a.game.local.p4.mail.open.as_ref().unwrap().0;

    // Forged parcel: the same slot twice -> refused, nothing taken.
    a.send(&C2S::Mail(MailC2S::Send { npc, to: "Destino".into(), subject: "x".into(), body: String::new(), gold: 0, items: vec![(slot, 4), (slot, 4)] }));
    step(&s, &mut a);
    assert_eq!(a.game.local.gold, gold0);

    // A real letter: 1000 gold + 4 units, postage 100.
    a.send(&C2S::Mail(MailC2S::Send { npc, to: "destino".into(), subject: "Regalo".into(), body: "Para el viaje".into(), gold: 1000, items: vec![(slot, 4)] }));
    a.wait_until("enviada", |b| says(b, "Carta enviada a Destino"));
    step(&s, &mut a);
    assert_eq!(a.game.local.gold, gold0 - 1100);
    assert_eq!(a.game.local.slot(slot).map(|i| i.amount), Some(3));
    b.wait_until("aviso", |b| says(b, "Te ha llegado una carta de Remitente"));

    // The recipient reads and takes it, once.
    let bgold = b.game.local.gold;
    b.send(&C2S::Interact { tx: CORREO.0, ty: CORREO.1 });
    b.wait_until("buzon b", |b| b.game.local.p4.mail.open.as_ref().is_some_and(|o| o.1.len() == 1));
    let letter = b.game.local.p4.mail.open.as_ref().unwrap().1[0].clone();
    assert!(letter.unread && letter.parcel && letter.sender == "Remitente");
    b.send(&C2S::Mail(MailC2S::Take { npc, id: letter.id }));
    b.send(&C2S::Mail(MailC2S::Take { npc, id: letter.id }));
    b.wait_until("paquete", |b| b.game.local.inventory.iter().flatten().any(|i| i.obj == 38));
    step(&s, &mut b);
    assert_eq!(b.game.local.gold, bgold + 1000, "taken once");
    assert_eq!(b.game.local.inventory.iter().flatten().filter(|i| i.obj == 38).map(|i| i.amount).sum::<u32>(), 4);

    // Clan vault: no clan, no vault.
    b.say(&format!("/ir 28 {} {}", BOVEDA.0, BOVEDA.1 + 1));
    b.wait_until("junto a la boveda", |b| (b.pos().0 - BOVEDA.0).abs() + (b.pos().1 - BOVEDA.1).abs() <= 3);
    b.send(&C2S::Interact { tx: BOVEDA.0, ty: BOVEDA.1 });
    b.wait_until("sin clan", |b| says(b, "Solo los miembros de un clan"));

    // Surgeon in Thir: refused from Tanaris, works next to him, faces of the race only.
    a.send(&C2S::Surgeon(SurgeonC2S::Change { npc: 1, head: 7 }));
    step(&s, &mut a);
    assert_ne!(a.me().head, 7);
    a.say(&format!("/ir 25 {} {}", CIRUJANO.0, CIRUJANO.1 + 1));
    a.wait_until("en Thir", |b| b.map() == 25);
    a.send(&C2S::Interact { tx: CIRUJANO.0, ty: CIRUJANO.1 });
    a.wait_until("cirujano", |b| b.game.local.p4.surgeon.open.is_some());
    let (snpc, heads, price, _) = a.game.local.p4.surgeon.open.clone().unwrap();
    assert!(heads.contains(&7) && !heads.contains(&101), "human male faces only");
    let g = a.game.local.gold;
    a.send(&C2S::Surgeon(SurgeonC2S::Change { npc: snpc, head: 7 }));
    a.wait_until("cara nueva", |b| b.me().head == 7);
    step(&s, &mut a);
    assert_eq!(a.game.local.gold, g - price);
}
