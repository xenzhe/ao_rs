//! Event system: duels, tournaments and auctions. OWNER: Opus D (docs/P4_CONTRATO.md).
//!
//! - Duels (retos): `ModRetos.bas` (`CrearReto`, `AceptarReto`, `CancelarSolicitudReto`,
//!   `BuscarSala`, `IniciarReto`, `iniciarRonda`, `MuereEnReto`, `ProcesarRondaGanada`,
//!   `FinalizarReto`, `AbandonarReto`, `SalaLiberada`, `PuedeReto(ConMensaje)`,
//!   `TodosPuedenReto`) with the rules of `Recursos/Dat/Retos.dat`.
//! - Auctions (subastas): `ModSubasta.bas` (`IniciarSubasta`, `FinalizarSubasta`,
//!   `DevolverItem`) and `Protocol.HandleOfertaInicial` / `HandleOfertaDeSubasta`.
//! - Tournaments (torneos): `ModTorneos.bas` (`IniciarTorneo`, `ParticiparTorneo`,
//!   `BorrarIndexInTorneo`, `ComenzarTorneoOk`, `ResetearTorneo`).
//!
//! Invasions (`ModInvasion.bas`) are off in the official data (`Eventos.dat`
//! `Cantidad=0`) and castles (`ModCastle.bas`) are player-placed emperor castles, not an
//! event: neither is ported (see the P4 report).
use crate::clock::GameTime;
use crate::world::event::{Duel, DuelConfig, DuelRequest, Fighter, Invitee, Lot, LotRow, RoomDef, Tournament};
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_formats::ini::Ini;
use ao_protocol::p4::event::{AuctionLot, DuelInvite, DuelView, EventView, TournamentSpec, TournamentView};
use ao_protocol::{EventC2S, EventS2C, Tone, S2C};
use tracing::warn;

/// `APUESTA_MAXIMA` (ModRetos.bas).
pub const MAX_BET: u32 = 100_000_000;
/// `OBJ_POCION_ROJA`.
pub const RED_POTION: u16 = 38;
/// `RONDAS_MAXIMAS` / `VICTORIAS_PARA_GANAR`.
pub const MAX_ROUNDS: u8 = 3;
pub const WINS_TO_WIN: u8 = 2;

/// `CostoPreSubasta` (Configuracion.ini): fee to publish, not refunded.
pub const AUCTION_FEE: u32 = 500;
/// `Descuento = MejorOferta / 100 * 5` (FinalizarSubasta): the house commission.
pub const AUCTION_COMMISSION_PERCENT: u64 = 5;
/// `SUBASTA_OFERTA_MAXIMA` (Declares.bas).
pub const AUCTION_MAX_PRICE: u32 = 1_999_999;
/// `MinutosDeSubasta = 5` in AO20; here the seller picks 1..=30 (5 by default).
pub const AUCTION_MAX_MINUTES: u8 = 30;
/// Bids in the last minute extend the lot by 30 s (`HandleOfertaDeSubasta`).
pub const AUCTION_SNIPE_SECS: u64 = 60;
pub const AUCTION_EXTEND_SECS: u64 = 30;
/// Several lots at once (AO20 had a single global one): a cap so the list stays readable.
pub const AUCTION_MAX_LOTS: usize = 30;
/// `Distancia(NpcList(TargetNPC).pos, .pos) > 2`.
pub const AUCTIONEER_RANGE: i32 = 2;
/// `e_Class` names in the order `IniciarTorneo` lists them.
pub fn class_name(class: u8) -> &'static str {
    const NAMES: [&str; 12] = ["Mago", "Clérigo", "Guerrero", "Asesino", "Bardo", "Druida", "Paladín", "Cazador", "Trabajador", "Pirata", "Ladrón", "Bandido"];
    NAMES.get((class as usize).wrapping_sub(1)).copied().unwrap_or("?")
}

/// Pure duel rules, kept apart for unit tests.
pub mod rules {
    /// `DiferenciaMinima`: 5 % of the best bid, at least 100; the first bid is the base price
    /// (AO20 compared against `MejorOferta = 0`, ignoring the base: fixed here).
    pub fn min_bid(base: u32, best: u32) -> u32 {
        if best == 0 {
            return base.max(1);
        }
        let step = ((best as u64 * 5) / 100).max(100);
        (best as u64 + step).min(u32::MAX as u64) as u32
    }

    /// What the seller receives: the price minus the 5 % commission.
    pub fn seller_share(price: u32) -> u32 {
        (price as u64 - price as u64 * super::AUCTION_COMMISSION_PERCENT / 100) as u32
    }

    /// `OroTotal = Apuesta * Jugadores * (1 - ImpuestoApuesta)`, in integer per-mille.
    pub fn pot_after_tax(pot: u64, tax: f64) -> u64 {
        let permille = (tax.clamp(0.0, 1.0) * 1000.0).round() as u64;
        pot * (1000 - permille) / 1000
    }

    /// `ProcesarRondaGanada`: the duel ends at round 3 or with 2 wins.
    pub fn duel_over(round: u8, wins_a: u8, wins_b: u8) -> bool {
        round >= super::MAX_ROUNDS || wins_a >= super::WINS_TO_WIN || wins_b >= super::WINS_TO_WIN
    }

    /// Value of a `Retos.dat` line: `5\t\t# N vs N` -> 5 (that file comments with `#`).
    pub fn dat_num(raw: Option<&String>) -> f64 {
        raw.map(|v| v.split('#').next().unwrap_or("").trim().replace(',', ".")).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    }
}

/// Who wins a finished duel.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Outcome {
    A,
    B,
    Draw,
}

impl World {
    /// (`on_event` is the network event loop; this is the game-events system.)
    pub(crate) fn on_game_event(&mut self, id: u32, msg: EventC2S) {
        match msg {
            EventC2S::Refresh => {
                self.send_event_state(id);
                self.send_lots(id);
            }
            EventC2S::DuelPropose { players, bet, max_potions } => self.duel_propose(id, players, bet, max_potions),
            EventC2S::DuelAccept { offerer } => self.duel_accept(id, &offerer),
            EventC2S::DuelCancel => self.duel_cancel(id),
            EventC2S::DuelQuit => {
                if self.player(id).is_some_and(|p| p.event.room.is_some()) {
                    self.duel_abandon(id, false);
                } else {
                    self.console(id, "No estás en un reto.", Tone::Info);
                }
            }
            EventC2S::AuctionPublish { slot, amount, base_price, minutes } => self.auction_publish(id, slot, amount, base_price, minutes),
            EventC2S::AuctionBid { lot, amount } => self.auction_bid(id, lot, amount),
            EventC2S::TournamentJoin => self.tournament_join(id),
            EventC2S::TournamentLeave => self.tournament_leave(id, true),
            EventC2S::TournamentOpen(spec) => self.tournament_open(id, spec),
            EventC2S::TournamentStart => self.tournament_start(id),
            EventC2S::TournamentClose => self.tournament_close(id),
        }
    }

    /// HOOK (combat): does `id` keep its items when dying (duels, tournaments, arenas)?
    /// Duels here never drop items (`CaenItems = False`, the default of `/RETO`).
    pub(crate) fn event_keeps_items_on_death(&self, id: u32) -> bool {
        self.player(id).is_some_and(|p| p.event.room.is_some())
    }

    /// HOOK (combat): `killer` (a player, or None) killed the player `victim`, after the
    /// death is processed. Only tells the room: the round is decided in `tick_event`
    /// (`MuereEnReto` -> `EquipoTieneVivos`), never in the middle of the attack.
    pub(crate) fn event_on_player_killed(&mut self, killer: Option<u32>, victim: u32) {
        let _ = killer;
        let Some(room) = self.player(victim).and_then(|p| p.event.room) else { return };
        let name = self.player(victim).map(|p| p.name.clone()).unwrap_or_default();
        for f in self.duel_fighters(room) {
            if f != victim {
                self.console(f, format!("{name} ha caído."), Tone::Combat);
            }
        }
    }

    /// Every world tick.
    pub(crate) fn tick_event(&mut self, now: GameTime) {
        if !self.events.lots_loaded {
            self.load_lots(now);
        }
        for room in 0..self.events.rooms.len() {
            self.tick_duel(room, now);
        }
        self.tick_auctions(now);
    }

    /// When the player enters the world (after everything else is sent).
    pub(crate) fn event_on_enter(&mut self, id: u32) {
        self.deliver_mailbox(id);
        self.send_event_state(id);
        self.send_lots(id);
    }

    /// Right before the player leaves the world, still present: requests are cancelled, a
    /// duel is lost by disconnection (`AbandonarReto(.., Desconexion:=True)`), and the
    /// fighter is revived and put back where they were, so they are saved alive outside
    /// the ring.
    pub(crate) fn event_on_leave(&mut self, id: u32) {
        let Some(name) = self.player(id).map(|p| p.name.clone()) else { return };
        self.events.queue.retain(|o| *o != id);
        if self.player(id).unwrap().event.request.is_some() {
            self.cancel_request(id, &format!("{name} se ha desconectado."));
        }
        for offerer in self.player_ids() {
            let listed = self.player(offerer).and_then(|p| p.event.request.as_ref()).is_some_and(|r| r.players.iter().any(|i| i.id == id));
            if listed && offerer != id {
                self.cancel_request(offerer, &format!("{name} se ha desconectado."));
            }
        }
        if self.player(id).unwrap().event.room.is_some() {
            self.duel_abandon(id, true);
        }
        if self.player(id).unwrap().event.in_tournament {
            self.tournament_leave(id, false);
        }
    }

    /// For the integrator: chat commands of AO20 (`/RETO`, `/ACEPTAR`, `/CANCELAR`,
    /// `/ABANDONAR`, `/OFERTAR`, `/PARTICIPAR`) mapped to the same actions as the window.
    /// Returns true if the line was one of them. Not wired yet (chat.rs is not ours).
    #[allow(dead_code)]
    pub(crate) fn event_command(&mut self, id: u32, line: &str) -> bool {
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("").to_ascii_uppercase();
        let rest: Vec<&str> = parts.collect();
        let msg = match cmd.as_str() {
            // /RETO <apuesta> <rival>[;compañero;rival2]
            "/RETO" if rest.len() >= 2 => {
                let bet = rest[0].parse().unwrap_or(0);
                let players = rest[1..].join(" ").split(';').map(|s| s.trim().to_string()).collect();
                EventC2S::DuelPropose { players, bet, max_potions: -1 }
            }
            "/ACEPTAR" if !rest.is_empty() => EventC2S::DuelAccept { offerer: rest.join(" ") },
            "/CANCELAR" => EventC2S::DuelCancel,
            "/ABANDONAR" => EventC2S::DuelQuit,
            "/OFERTAR" if rest.len() == 2 => EventC2S::AuctionBid { lot: rest[0].parse().unwrap_or(0), amount: rest[1].parse().unwrap_or(0) },
            "/PARTICIPAR" => EventC2S::TournamentJoin,
            _ => return false,
        };
        self.on_game_event(id, msg);
        true
    }

    // ================================================================== helpers

    fn id_by_name(&self, name: &str) -> Option<u32> {
        let name = name.trim();
        self.player_ids().into_iter().find(|id| self.player(*id).is_some_and(|p| p.name.eq_ignore_ascii_case(name)))
    }

    fn name_of(&self, id: u32) -> String {
        self.player(id).map(|p| p.name.clone()).unwrap_or_default()
    }

    fn announce(&self, text: impl Into<String>) {
        self.broadcast_all(&S2C::Console { text: text.into(), color: Tone::Info }, None);
    }

    fn event_send_gold(&self, id: u32) {
        self.send_inventory_vitals(id);
    }

    /// Saves an account character right away. Auction lots and the mailbox live in the
    /// database: when an item or gold moves between them and the character, both sides are
    /// written at once, so a crash cannot leave the same thing in both (or in neither).
    fn event_save(&self, id: u32) {
        if let Some(p) = self.player(id) {
            self.save_player(p);
        }
    }

    /// Who may run GM-only events. GM roles belong to the GM system (Opus C), which is not
    /// merged: development servers allow anyone; the integrator swaps this for its role check.
    fn event_is_staff(&self, id: u32) -> bool {
        let _ = id;
        self.cfg.dev_login
    }

    fn red_potions(&self, id: u32) -> u32 {
        self.player(id).map_or(0, |p| p.inv.slots.iter().flatten().filter(|i| i.obj == RED_POTION).map(|i| i.amount).sum())
    }

    /// Revives (if dead), heals and fills mana (`RevivirYLimpiar`).
    fn revive_and_clean(&mut self, id: u32) {
        if self.player(id).is_none() {
            return;
        }
        if self.is_dead(id) {
            self.resurrect(id);
        }
        let p = self.player_mut(id);
        let s = &mut p.status;
        s.paralyzed_until = None;
        s.immobilized_until = None;
        s.poisoned = false;
        s.poisoned_until = None;
        s.blind_until = None;
        s.dumb_until = None;
        s.invisible_until = None;
        s.eots.clear();
        p.mana = p.max_mana;
        let max = p.max_hp;
        self.heal_player(id, max);
    }

    /// `PuedeReto` (silent) / `PuedeRetoConMensaje`.
    fn can_duel(&self, id: u32, tell: bool) -> bool {
        let Some(p) = self.player(id) else { return false };
        let why = if p.event.room.is_some() {
            Some("Ya te encuentras en un reto.")
        } else if p.event.in_tournament {
            Some("No puedes ir a un reto si participas de un torneo.")
        } else if self.is_dead(id) {
            Some("¡¡Estás muerto!!")
        } else if !self.maps.get(&p.map).is_some_and(|m| m.map.safe) {
            Some("No puedes participar de un reto en un mapa inseguro.")
        } else {
            None
        };
        if let (Some(w), true) = (why, tell) {
            self.console(id, w, Tone::Warn);
        }
        why.is_none()
    }

    /// `Retos.dat`, read once.
    fn duel_config(&mut self) -> DuelConfig {
        if let Some(c) = &self.events.duel_cfg {
            return c.clone();
        }
        let ini = Ini::load(&self.recursos.join("Dat").join("Retos.dat"));
        let r = ini.section("Retos");
        let s = ini.section("Salas");
        let get = |sec: Option<&std::collections::HashMap<String, String>>, k: &str| rules::dat_num(sec.and_then(|m| m.get(k)));
        let (w, h) = (get(s, "ancho") as i32, get(s, "alto") as i32);
        let rooms = (1..=get(s, "cantidad") as usize)
            .filter_map(|i| {
                let sec = ini.section(&format!("Sala{i}"));
                let (map, x, y) = (get(sec, "mapa") as u16, get(sec, "x") as i32, get(sec, "y") as i32);
                (map > 0 && x > 0 && y > 0).then_some(RoomDef { map, left: (x, y), right: (x + w - 1, y + h - 1) })
            })
            .collect::<Vec<_>>();
        if rooms.is_empty() {
            warn!("Retos.dat sin salas: los retos no podrán empezar");
        }
        let c = DuelConfig {
            max_team: (get(r, "maximoequipo") as usize).max(1),
            min_bet: get(r, "apuestaminima") as u32,
            tax: get(r, "impuestoapuesta"),
            max_secs: (get(r, "duracionmaxima") as u64).max(60),
            countdown_secs: get(r, "tiempoconteo") as u64,
            rooms,
        };
        self.events.rooms = vec![None; c.rooms.len()];
        self.events.duel_cfg = Some(c.clone());
        c
    }

    // ================================================================== state to clients

    fn invite_of(&self, offerer: u32) -> Option<DuelInvite> {
        let p = self.player(offerer)?;
        let r = p.event.request.as_ref()?;
        let mut team_a = vec![p.name.clone()];
        let mut team_b = Vec::new();
        for (i, inv) in r.players.iter().enumerate() {
            if i % 2 == 1 { team_a.push(inv.name.clone()) } else { team_b.push(inv.name.clone()) }
        }
        let missing = r.players.iter().filter(|i| !i.accepted).map(|i| i.name.clone()).collect();
        Some(DuelInvite { offerer: p.name.clone(), team_a, team_b, bet: r.bet, max_potions: r.max_potions, missing, queued: r.queued })
    }

    fn event_view(&self, id: u32) -> EventView {
        let Some(me) = self.player(id) else { return EventView::default() };
        let mut v = EventView::default();
        if let Some(c) = &self.events.duel_cfg {
            v.min_bet = c.min_bet;
            v.max_team = c.max_team as u8;
            v.tax_percent = (c.tax * 100.0).round() as u8;
        }
        for o in self.player_ids() {
            let Some(r) = self.player(o).and_then(|p| p.event.request.as_ref()) else { continue };
            if r.players.iter().any(|i| i.id == id && !i.accepted) {
                v.invites.extend(self.invite_of(o));
            }
        }
        v.pending = if me.event.request.is_some() { self.invite_of(id) } else { me.event.accepted.and_then(|o| self.invite_of(o)) };
        if let Some(d) = me.event.room.and_then(|r| self.events.rooms.get(r)).and_then(|d| d.as_ref()) {
            let now = self.now();
            v.duel = Some(DuelView {
                team_a: d.a.iter().map(|f| f.name.clone()).collect(),
                team_b: d.b.iter().map(|f| f.name.clone()).collect(),
                round: d.round,
                wins_a: d.wins_a,
                wins_b: d.wins_b,
                mine_a: d.a.iter().any(|f| f.id == Some(id)),
                bet: d.bet,
                countdown: d.fight_at.since(now).div_ceil(1000) as u16,
                time_left: (d.ends_at.since(now) / 1000) as u32,
            });
        }
        v.tournament = self.events.tournament.as_ref().map(|t| TournamentView {
            spec: t.spec.clone(),
            signed: t.signed.iter().map(|(_, n)| n.clone()).collect(),
            me_signed: me.event.in_tournament,
            started: t.started,
        });
        v
    }

    pub(crate) fn send_event_state(&self, id: u32) {
        if self.player(id).is_some() {
            self.send(id, &S2C::Event(EventS2C::State(Box::new(self.event_view(id)))));
        }
    }

    fn send_states(&self, ids: impl IntoIterator<Item = u32>) {
        for id in ids {
            self.send_event_state(id);
        }
    }

    /// Everyone in a request: the offerer and the invitees.
    fn request_members(&self, offerer: u32) -> Vec<u32> {
        let mut v = vec![offerer];
        if let Some(r) = self.player(offerer).and_then(|p| p.event.request.as_ref()) {
            v.extend(r.players.iter().map(|i| i.id));
        }
        v
    }

    fn duel_fighters(&self, room: usize) -> Vec<u32> {
        self.events.rooms.get(room).and_then(|d| d.as_ref()).map_or_else(Vec::new, |d| d.a.iter().chain(&d.b).filter_map(|f| f.id).collect())
    }

    fn lot_views(&self) -> Vec<AuctionLot> {
        let now = self.now();
        self.events
            .lots
            .iter()
            .map(|l| AuctionLot {
                id: l.id,
                seller: l.seller.clone(),
                obj: l.obj,
                amount: l.amount,
                base_price: l.base_price,
                best: l.best,
                bidder: l.bidder.clone().unwrap_or_default(),
                min_bid: rules::min_bid(l.base_price, l.best),
                secs_left: (l.ends_at.since(now) / 1000) as u32,
            })
            .collect()
    }

    fn send_lots(&self, id: u32) {
        self.send(id, &S2C::Event(EventS2C::Auctions { lots: self.lot_views() }));
    }

    fn broadcast_lots(&self) {
        self.broadcast_all(&S2C::Event(EventS2C::Auctions { lots: self.lot_views() }), None);
    }

    // ================================================================== duels: requests

    /// `CrearReto`.
    fn duel_propose(&mut self, id: u32, players: Vec<String>, bet: u32, max_potions: i16) {
        let cfg = self.duel_config();
        let me = self.name_of(id);
        // A new request replaces the previous one (or the previous acceptance).
        if self.player(id).unwrap().event.request.is_some() {
            self.cancel_request(id, &format!("{me} ha cancelado la solicitud."));
        } else if let Some(o) = self.player(id).unwrap().event.accepted {
            self.cancel_request(o, &format!("{me} ha cancelado su admisión."));
        }
        let players: Vec<String> = players.into_iter().map(|s| s.trim().to_string()).collect();
        let max_len = cfg.max_team * 2 - 1;
        if players.is_empty() || players.len() > max_len || players.len().is_multiple_of(2) || players.iter().any(|n| n.is_empty() || n.len() > 30) {
            return self.console(id, format!("Indica a tu rival (y compañeros): hasta {} vs {}.", cfg.max_team, cfg.max_team), Tone::Warn);
        }
        // NombreRepetidoEnReto
        for (i, n) in players.iter().enumerate() {
            if n.eq_ignore_ascii_case(&me) || players[i + 1..].iter().any(|o| o.eq_ignore_ascii_case(n)) {
                return self.console(id, format!("El usuario {n} no puede jugar un reto en este momento."), Tone::Warn);
            }
        }
        if bet < cfg.min_bet || bet > MAX_BET {
            return self.console(id, format!("La apuesta mínima es de {} monedas de oro.", cfg.min_bet), Tone::Warn);
        }
        if !self.can_duel(id, true) {
            return;
        }
        if self.player(id).unwrap().inv.gold < bet {
            return self.console(id, "No tienes suficiente oro.", Tone::Warn);
        }
        if max_potions >= 0 && self.red_potions(id) > max_potions as u32 {
            return self.console(id, format!("Tienes demasiadas pociones rojas (Cantidad máxima: {max_potions})."), Tone::Warn);
        }
        let mut invitees = Vec::new();
        for n in &players {
            match self.id_by_name(n) {
                Some(t) if self.can_duel(t, false) => invitees.push(Invitee { name: self.name_of(t), id: t, accepted: false }),
                _ => return self.console(id, format!("El usuario {n} no puede jugar un reto en este momento."), Tone::Warn),
            }
        }
        let req = DuelRequest { bet, max_potions, players: invitees, queued: false };
        self.player_mut(id).event.request = Some(req);
        let inv = self.invite_of(id).unwrap();
        let level = self.player(id).unwrap().level;
        let mut terms = format!("{} vs {}. Apuesta: {bet} monedas de oro.", inv.team_a.join(", "), inv.team_b.join(", "));
        if max_potions >= 0 {
            terms.push_str(&format!(" Máximo {max_potions} pociones rojas."));
        }
        for t in self.request_members(id).into_iter().skip(1) {
            self.console(t, format!("{me}({level}) te invita a jugar el siguiente reto:"), Tone::Info);
            self.console(t, terms.clone(), Tone::Info);
            self.console(t, format!("Acepta el reto de {me} en la ventana de eventos (V)."), Tone::Info);
        }
        self.console(id, "Has enviado una solicitud para el siguiente reto:", Tone::Info);
        self.console(id, terms, Tone::Info);
        let members = self.request_members(id);
        self.send_states(members);
    }

    /// `AceptarReto`. Accepting the same request twice is refused (AO20 cancelled it).
    fn duel_accept(&mut self, id: u32, offerer_name: &str) {
        if !self.can_duel(id, true) {
            return;
        }
        let me = self.name_of(id);
        let Some(offerer) = self.id_by_name(offerer_name) else {
            return self.console(id, format!("{} no está conectado.", offerer_name.trim()), Tone::Warn);
        };
        if self.player(id).unwrap().event.accepted == Some(offerer) {
            return self.console(id, "Ya has aceptado este reto.", Tone::Info);
        }
        if self.player(id).unwrap().event.request.is_some() {
            self.cancel_request(id, &format!("{me} ha cancelado la solicitud."));
        } else if let Some(o) = self.player(id).unwrap().event.accepted {
            self.cancel_request(o, &format!("{me} ha cancelado su admisión."));
        }
        let oname = self.name_of(offerer);
        let found = self.player(offerer).and_then(|p| p.event.request.as_ref()).filter(|r| !r.queued).and_then(|r| r.players.iter().position(|i| i.id == id && !i.accepted).map(|i| (i, r.bet, r.max_potions)));
        let Some((index, bet, max_potions)) = found else {
            return self.console(id, format!("{oname} no te ha invitado a ningún reto o ha sido cancelado."), Tone::Warn);
        };
        if self.player(id).unwrap().inv.gold < bet {
            return self.console(id, format!("Necesitas al menos {bet} monedas de oro para aceptar este reto."), Tone::Warn);
        }
        if max_potions >= 0 && self.red_potions(id) > max_potions as u32 {
            return self.console(id, format!("Tienes demasiadas pociones rojas (Cantidad máxima: {max_potions})."), Tone::Warn);
        }
        self.player_mut(offerer).event.request.as_mut().unwrap().players[index].accepted = true;
        self.player_mut(id).event.accepted = Some(offerer);
        self.console(id, format!("Has aceptado el reto de {oname}."), Tone::Good);
        let members = self.request_members(offerer);
        for m in &members {
            if *m != id {
                self.console(*m, format!("{me} ha aceptado el reto."), Tone::Info);
            }
        }
        let missing = self.invite_of(offerer).map(|i| i.missing).unwrap_or_default();
        if missing.is_empty() {
            for m in &members {
                self.console(*m, "Todos los jugadores han aceptado el reto. Buscando sala...", Tone::Info);
            }
            self.find_room(offerer);
        } else {
            for m in &members {
                self.console(*m, format!("Faltan aceptar: {}", missing.join(" - ")), Tone::Info);
            }
        }
        let members = self.request_members(offerer);
        self.send_states(members);
        self.send_event_state(offerer);
    }

    /// `HandleCancelDuel`.
    fn duel_cancel(&mut self, id: u32) {
        let me = self.name_of(id);
        if self.player(id).unwrap().event.request.is_some() {
            self.cancel_request(id, &format!("{me} ha cancelado la solicitud."));
        } else if let Some(o) = self.player(id).unwrap().event.accepted {
            self.cancel_request(o, &format!("{me} ha cancelado su admisión."));
        } else {
            self.console(id, "No tienes ningún reto pendiente.", Tone::Info);
        }
    }

    /// `CancelarSolicitudReto`: nothing was taken yet, so nothing is returned.
    fn cancel_request(&mut self, offerer: u32, why: &str) {
        let Some(req) = self.player_mut(offerer).event.request.take() else { return };
        self.events.queue.retain(|o| *o != offerer);
        for i in &req.players {
            if self.player(i.id).is_some_and(|p| p.name == i.name) {
                if self.player(i.id).unwrap().event.accepted == Some(offerer) {
                    self.player_mut(i.id).event.accepted = None;
                }
                self.console(i.id, why, Tone::Warn);
                self.console(i.id, "El reto ha sido cancelado.", Tone::Warn);
                self.send_event_state(i.id);
            }
        }
        self.console(offerer, why, Tone::Warn);
        self.console(offerer, "El reto ha sido cancelado.", Tone::Warn);
        self.send_event_state(offerer);
    }

    /// `BuscarSala`: a random free room, or the waiting list.
    fn find_room(&mut self, offerer: u32) {
        self.duel_config();
        let free: Vec<usize> = (0..self.events.rooms.len()).filter(|r| self.events.rooms[*r].is_none()).collect();
        if free.is_empty() {
            self.player_mut(offerer).event.request.as_mut().unwrap().queued = true;
            self.events.queue.push_back(offerer);
            for m in self.request_members(offerer) {
                self.console(m, "No hay salas disponibles. El reto comenzará cuando se desocupe una sala.", Tone::Info);
            }
            return;
        }
        let pick = free[self.rng.range(0, free.len() as i32 - 1) as usize];
        self.start_duel(offerer, pick);
    }

    /// `TodosPuedenReto` + `IniciarReto`: checks everybody again, then takes every bet.
    fn start_duel(&mut self, offerer: u32, room: usize) {
        let Some(req) = self.player(offerer).and_then(|p| p.event.request.clone()) else { return };
        let mut ids = vec![offerer];
        for i in &req.players {
            if !self.player(i.id).is_some_and(|p| p.name == i.name) || self.player(i.id).unwrap().event.accepted != Some(offerer) {
                return self.cancel_request(offerer, &format!("{} no puede entrar al reto en este momento.", i.name));
            }
            ids.push(i.id);
        }
        for t in &ids {
            let name = self.name_of(*t);
            if !self.can_duel(*t, false) {
                return self.cancel_request(offerer, &format!("{name} no puede entrar al reto en este momento."));
            }
            if self.player(*t).unwrap().inv.gold < req.bet {
                return self.cancel_request(offerer, &format!("{name} no tiene las monedas de oro suficientes."));
            }
            if req.max_potions >= 0 && self.red_potions(*t) > req.max_potions as u32 {
                return self.cancel_request(offerer, &format!("{name} tiene demasiadas pociones rojas (Cantidad máxima: {}).", req.max_potions));
            }
        }
        let cfg = self.duel_config();
        let now = self.now();
        let fighter = |w: &World, id: u32| {
            let p = w.player(id).unwrap();
            Fighter { name: p.name.clone(), id: Some(id), back: (p.map, p.x, p.y) }
        };
        let mut a = vec![fighter(self, offerer)];
        let mut b = Vec::new();
        for (i, t) in ids.iter().skip(1).enumerate() {
            if i % 2 == 1 { a.push(fighter(self, *t)) } else { b.push(fighter(self, *t)) }
        }
        self.player_mut(offerer).event.request = None;
        let mut pot = 0u64;
        for t in &ids {
            let p = self.player_mut(*t);
            p.inv.gold -= req.bet;
            p.event.accepted = None;
            p.event.room = Some(room);
            pot += req.bet as u64;
            self.event_send_gold(*t);
            self.console(*t, format!("Otorgas {} monedas de oro al pozo del reto.", req.bet), Tone::Info);
            self.console(*t, "¡Ha comenzado el reto!", Tone::Good);
            self.console(*t, "Para admitir la derrota usa «Abandonar» en la ventana de eventos (V).", Tone::Info);
        }
        self.events.rooms[room] = Some(Duel { room, a, b, bet: req.bet, pot, round: 1, wins_a: 0, wins_b: 0, fight_at: now, ends_at: now.after_secs(cfg.max_secs), forfeit: None });
        self.begin_round(room);
    }

    // ================================================================== duels: rounds

    /// `iniciarRonda`: everyone revived and placed at their corner; the countdown holds them.
    fn begin_round(&mut self, room: usize) {
        let cfg = self.duel_config();
        let def = cfg.rooms[room];
        let now = self.now();
        let Some(d) = self.events.rooms[room].as_mut() else { return };
        d.fight_at = now.after_secs(cfg.countdown_secs);
        let (round, fight_at) = (d.round, d.fight_at);
        let a: Vec<u32> = d.a.iter().filter_map(|f| f.id).collect();
        let b: Vec<u32> = d.b.iter().filter_map(|f| f.id).collect();
        for (team, ids) in [(true, a), (false, b)] {
            // Sides swap every round (`(Ronda + indiceJugador) Mod 2`).
            let (x, y) = if team == (round % 2 == 1) { def.left } else { def.right };
            for id in ids {
                self.revive_and_clean(id);
                self.teleport(id, def.map, x, y);
                if fight_at > now {
                    self.player_mut(id).status.immobilized_until = Some(fight_at);
                }
                self.console(id, format!("Comienza la ronda Nº{round}"), Tone::Info);
            }
        }
        let f = self.duel_fighters(room);
        self.send_states(f);
    }

    fn tick_duel(&mut self, room: usize, now: GameTime) {
        let Some(d) = self.events.rooms[room].as_ref() else { return };
        if let Some(a_lost) = d.forfeit {
            return self.finish_duel(room, if a_lost { Outcome::B } else { Outcome::A }, false);
        }
        if now >= d.ends_at {
            let o = match d.wins_a.cmp(&d.wins_b) {
                std::cmp::Ordering::Greater => Outcome::A,
                std::cmp::Ordering::Less => Outcome::B,
                std::cmp::Ordering::Equal => Outcome::Draw,
            };
            return self.finish_duel(room, o, true);
        }
        let fight_at = d.fight_at;
        let (a, b): (Vec<u32>, Vec<u32>) = (d.a.iter().filter_map(|f| f.id).collect(), d.b.iter().filter_map(|f| f.id).collect());
        if now < fight_at {
            // VB6 only told the client to stop (`WriteStopped`): the server holds them.
            for id in a.iter().chain(&b) {
                self.player_mut(*id).status.immobilized_until = Some(fight_at);
            }
            return;
        }
        if now.since(fight_at) < self.cfg.world.tick_ms.max(1) {
            for id in a.iter().chain(&b) {
                self.console(*id, "¡A pelear!", Tone::Good);
                self.send_event_state(*id);
            }
        }
        let alive = |w: &World, ids: &[u32]| ids.iter().any(|id| !w.is_dead(*id));
        match (alive(self, &a), alive(self, &b)) {
            (true, true) => {}
            // Both teams down in the same tick: the round is played again.
            (false, false) => self.begin_round(room),
            (a_alive, _) => self.round_won(room, a_alive),
        }
    }

    /// `ProcesarRondaGanada`.
    fn round_won(&mut self, room: usize, a_won: bool) {
        let d = self.events.rooms[room].as_mut().unwrap();
        if a_won { d.wins_a += 1 } else { d.wins_b += 1 }
        if rules::duel_over(d.round, d.wins_a, d.wins_b) {
            let o = match d.wins_a.cmp(&d.wins_b) {
                std::cmp::Ordering::Greater => Outcome::A,
                std::cmp::Ordering::Less => Outcome::B,
                std::cmp::Ordering::Equal => Outcome::Draw,
            };
            return self.finish_duel(room, o, false);
        }
        d.round += 1;
        let names: Vec<String> = if a_won { &d.a } else { &d.b }.iter().filter(|f| f.id.is_some()).map(|f| f.name.clone()).collect();
        for id in self.duel_fighters(room) {
            self.console(id, format!("Esta ronda es para {}.", names.join(" y ")), Tone::Info);
        }
        self.begin_round(room);
    }

    /// `FinalizarReto`: pays the pot (minus `ImpuestoApuesta`) to the winners present, or
    /// shares it out in a draw, and sends everybody back. The room is freed once.
    fn finish_duel(&mut self, room: usize, outcome: Outcome, timeout: bool) {
        let Some(d) = self.events.rooms[room].take() else { return };
        let cfg = self.duel_config();
        let net = rules::pot_after_tax(d.pot, cfg.tax);
        let present = |team: &[Fighter]| team.iter().filter_map(|f| f.id.map(|id| (id, f.back))).collect::<Vec<_>>();
        let (pa, pb) = (present(&d.a), present(&d.b));
        let total_players = (d.a.len() + d.b.len()) as u64;
        // Draw: `oroTotal \ (UBound(.Jugadores) + 1)` to each one still there.
        let (winners, share): (Vec<u32>, u64) = match outcome {
            Outcome::Draw => (pa.iter().chain(&pb).map(|x| x.0).collect(), net / total_players.max(1)),
            Outcome::A => (pa.iter().map(|x| x.0).collect(), net / (pa.len() as u64).max(1)),
            Outcome::B => (pb.iter().map(|x| x.0).collect(), net / (pb.len() as u64).max(1)),
        };
        let mut paid = 0u64;
        for id in &winners {
            let p = self.player_mut(*id);
            let add = share.min((u32::MAX - p.inv.gold) as u64) as u32;
            p.inv.gold += add;
            paid += add as u64;
            self.event_send_gold(*id);
            self.console(*id, format!("Has ganado {share} monedas de oro."), Tone::Good);
        }
        self.events.house_gold += d.pot - paid;
        for (id, back) in pa.iter().chain(&pb) {
            if timeout {
                self.console(*id, "Se ha agotado el tiempo del reto.", Tone::Info);
            }
            self.revive_and_clean(*id);
            self.player_mut(*id).event.room = None;
            self.teleport(*id, back.0, back.1, back.2);
            self.send_event_state(*id);
        }
        let names = |team: &[Fighter]| team.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(" y ");
        let (na, nb) = (names(&d.a), names(&d.b));
        match outcome {
            Outcome::Draw => self.announce(format!("Retos » {na} vs {nb}. Ninguno pudo vencer a su rival.")),
            o => {
                let (w, l) = if o == Outcome::A { (na, nb) } else { (nb, na) };
                if d.a.len() > 1 {
                    self.announce(format!("Retos » El equipo {w} venció al equipo {l} y se quedó con el botín de: {} monedas de oro.", d.bet));
                } else {
                    self.announce(format!("Retos » {w} venció a {l} y se quedó con el botín de: {} monedas de oro.", d.bet));
                }
            }
        }
        self.room_freed(room);
    }

    /// `SalaLiberada`: the next valid request of the waiting list takes the room.
    fn room_freed(&mut self, room: usize) {
        while self.events.rooms[room].is_none() {
            let Some(o) = self.events.queue.pop_front() else { break };
            if self.player(o).and_then(|p| p.event.request.as_ref()).is_some_and(|r| r.queued) {
                self.player_mut(o).event.request.as_mut().unwrap().queued = false;
                self.start_duel(o, room);
            }
        }
    }

    /// `AbandonarReto`: the leaver loses their bet (it stays in the pot), is revived and
    /// sent back. A team left empty loses the duel at once.
    fn duel_abandon(&mut self, id: u32, disconnected: bool) {
        let Some(room) = self.player(id).and_then(|p| p.event.room) else { return };
        let name = self.name_of(id);
        let Some(d) = self.events.rooms.get_mut(room).and_then(|d| d.as_mut()) else {
            self.player_mut(id).event.room = None;
            return;
        };
        let in_a = d.a.iter().any(|f| f.id == Some(id));
        let mut back = None;
        for f in d.a.iter_mut().chain(d.b.iter_mut()) {
            if f.id == Some(id) {
                f.id = None;
                back = Some(f.back);
            }
        }
        let team_empty = if in_a { d.a.iter().all(|f| f.id.is_none()) } else { d.b.iter().all(|f| f.id.is_none()) };
        if team_empty {
            d.forfeit = Some(in_a);
        }
        self.player_mut(id).event.room = None;
        self.revive_and_clean(id);
        if let Some((m, x, y)) = back {
            self.teleport(id, m, x, y);
        }
        if !disconnected {
            self.console(id, "Has abandonado el reto.", Tone::Warn);
        }
        let text = if disconnected { format!("{name} es descalificado por desconectarse.") } else { format!("{name} ha abandonado el reto.") };
        for f in self.duel_fighters(room) {
            self.console(f, text.clone(), Tone::Warn);
        }
        self.send_event_state(id);
        if team_empty {
            let o = if in_a { Outcome::B } else { Outcome::A };
            self.finish_duel(room, o, false);
        }
    }

    // ================================================================== auctions

    /// Restores the open lots after a restart: their time left starts counting again.
    fn load_lots(&mut self, now: GameTime) {
        self.events.lots_loaded = true;
        match self.db.load_lots() {
            Ok(rows) => {
                for r in rows {
                    self.events.next_lot = self.events.next_lot.max(r.id);
                    self.events.lots.push(Lot { id: r.id, seller: r.seller, obj: r.obj, amount: r.amount, base_price: r.base_price, best: r.best, bidder: r.bidder, ends_at: now.after_secs(r.secs_left as u64) });
                }
            }
            Err(e) => warn!(?e, "no se pudieron leer las subastas"),
        }
    }

    fn save_lot(&self, l: &Lot) {
        let row = LotRow { id: l.id, seller: l.seller.clone(), obj: l.obj, amount: l.amount, base_price: l.base_price, best: l.best, bidder: l.bidder.clone(), secs_left: (l.ends_at.since(self.now()) / 1000) as u32 };
        if let Err(e) = self.db.save_lot(&row) {
            warn!(?e, lot = l.id, "no se pudo guardar la subasta");
        }
    }

    /// Next to an auctioneer (`npcType = Subastador`, at most 2 tiles away).
    fn near_auctioneer(&self, id: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        self.maps.get(&p.map).is_some_and(|m| m.npcs.values().any(|n| n.alive() && n.kind == NpcType::Auctioneer && (n.x - p.x).abs() + (n.y - p.y).abs() <= AUCTIONEER_RANGE))
    }

    /// `IniciarSubasta` + `HandleOfertaInicial`, in one step and from the inventory (AO20
    /// took the object lying under the seller and asked the price in a second message).
    fn auction_publish(&mut self, id: u32, slot: u8, amount: u32, base_price: u32, minutes: u8) {
        if self.is_dead(id) {
            return self.console(id, "¡¡Estás muerto!!", Tone::Warn);
        }
        // AO20 asked for a click on the auctioneer, but no official map places one
        // (npcs.dat 'Subastador' appears in none of the 842 maps): a city works too.
        let in_city = self.player(id).is_some_and(|p| self.maps.get(&p.map).is_some_and(|m| m.map.safe));
        if !in_city && !self.near_auctioneer(id) {
            return self.console(id, "Solo puedes subastar en una ciudad o junto al subastador.", Tone::Warn);
        }
        let me = self.name_of(id);
        if self.events.lots.iter().any(|l| l.seller.eq_ignore_ascii_case(&me)) {
            return self.console(id, "Ya estás subastando un objeto.", Tone::Warn);
        }
        if self.events.lots.len() >= AUCTION_MAX_LOTS {
            return self.console(id, "La casa de subastas está llena, vuelve más tarde.", Tone::Warn);
        }
        if !(1..=AUCTION_MAX_PRICE).contains(&base_price) {
            return self.console(id, format!("El precio inicial debe estar entre 1 y {AUCTION_MAX_PRICE} monedas."), Tone::Warn);
        }
        if !(1..=AUCTION_MAX_MINUTES).contains(&minutes) {
            return self.console(id, format!("La subasta dura entre 1 y {AUCTION_MAX_MINUTES} minutos."), Tone::Warn);
        }
        let p = self.player(id).unwrap();
        let Some(item) = p.inv.get(slot) else {
            return self.console(id, "No tienes ese objeto.", Tone::Warn);
        };
        if amount == 0 || amount > item.amount {
            return self.console(id, "No tienes esa cantidad.", Tone::Warn);
        }
        if p.inv.equipped.is_equipped(slot) {
            return self.console(id, "Desequipa el objeto antes de subastarlo.", Tone::Warn);
        }
        if self.ao.objs.get(item.obj as u32).is_none_or(|o| o.auctionable != 1) {
            return self.console(id, "Ese objeto no se puede subastar.", Tone::Warn);
        }
        if p.inv.gold < AUCTION_FEE {
            return self.console(id, format!("Necesitas {AUCTION_FEE} monedas de oro para subastar."), Tone::Warn);
        }
        self.player_mut(id).inv.gold -= AUCTION_FEE;
        self.events.house_gold += AUCTION_FEE as u64;
        self.event_send_gold(id);
        self.remove_from_slot(id, slot, amount);
        self.events.next_lot += 1;
        let lot = Lot { id: self.events.next_lot, seller: me.clone(), obj: item.obj, amount, base_price, best: 0, bidder: None, ends_at: self.now().after_secs(minutes as u64 * 60) };
        self.save_lot(&lot);
        self.event_save(id);
        let obj_name = self.ao.objs.get(item.obj as u32).map(|o| o.name.clone()).unwrap_or_default();
        self.events.lots.push(lot);
        self.announce(format!("{me} está subastando: {obj_name} (Cantidad: {amount}) - con un precio inicial de {base_price} monedas. Puja en la casa de subastas (U)."));
        self.broadcast_lots();
    }

    /// `HandleOfertaDeSubasta`: the bid is held by the server; the previous bidder gets theirs
    /// back at once (by mail if offline: AO20 dereferenced them without checking).
    fn auction_bid(&mut self, id: u32, lot_id: u32, amount: u32) {
        let me = self.name_of(id);
        let Some(i) = self.events.lots.iter().position(|l| l.id == lot_id) else {
            return self.console(id, "No hay ninguna subasta en curso con ese número.", Tone::Warn);
        };
        let l = &self.events.lots[i];
        if l.seller.eq_ignore_ascii_case(&me) {
            return self.console(id, "No podés auto ofertar en tus subastas.", Tone::Warn);
        }
        let min = rules::min_bid(l.base_price, l.best);
        if amount < min {
            return self.console(id, format!("La oferta mínima es de {min} monedas de oro."), Tone::Warn);
        }
        if self.player(id).unwrap().inv.gold < amount {
            return self.console(id, "No posees esa cantidad de oro.", Tone::Warn);
        }
        self.player_mut(id).inv.gold -= amount;
        self.event_send_gold(id);
        let now = self.now();
        let l = &mut self.events.lots[i];
        let previous = l.bidder.replace(me.clone()).map(|b| (b, l.best));
        l.best = amount;
        let extended = l.ends_at.since(now) < AUCTION_SNIPE_SECS * 1000;
        if extended {
            l.ends_at = l.ends_at.after_secs(AUCTION_EXTEND_SECS);
        }
        let (obj, lot) = (l.obj, l.clone());
        self.save_lot(&lot);
        self.event_save(id);
        if let Some((who, gold)) = previous {
            self.pay(&who, gold, "Te superaron en una subasta: te devolvemos tu oferta.");
        }
        let obj_name = self.ao.objs.get(obj as u32).map(|o| o.name.clone()).unwrap_or_default();
        self.console(id, format!("Has ofrecido {amount} monedas de oro por {obj_name}."), Tone::Good);
        if extended {
            self.announce(format!("Oferta mejorada por: {me} (Ofrece {amount} monedas de oro) - Tiempo Extendido."));
        } else {
            self.announce(format!("{me} ofrece {amount} monedas de oro por {obj_name}."));
        }
        self.broadcast_lots();
    }

    fn tick_auctions(&mut self, now: GameTime) {
        let due: Vec<u32> = self.events.lots.iter().filter(|l| now >= l.ends_at).map(|l| l.id).collect();
        for id in due {
            self.close_lot(id);
        }
        // Time left is saved every 10 s, so a restart loses at most that.
        let sec = now.ms() / 10_000;
        if sec != self.events.lots_saved_at {
            self.events.lots_saved_at = sec;
            for l in &self.events.lots {
                self.save_lot(l);
            }
        }
    }

    /// `FinalizarSubasta` (sold: item to the buyer, price minus 5 % to the seller) or
    /// `DevolverItem` (no bids: back to the seller).
    fn close_lot(&mut self, lot_id: u32) {
        let Some(i) = self.events.lots.iter().position(|l| l.id == lot_id) else { return };
        let l = self.events.lots.remove(i);
        if let Err(e) = self.db.delete_lot(l.id) {
            warn!(?e, lot = l.id, "no se pudo borrar la subasta");
        }
        let obj_name = self.ao.objs.get(l.obj as u32).map(|o| o.name.clone()).unwrap_or_default();
        match &l.bidder {
            Some(buyer) => {
                self.deliver(buyer, l.obj, l.amount, "¡Has ganado la subasta!");
                let share = rules::seller_share(l.best);
                self.events.house_gold += (l.best - share) as u64;
                self.pay(&l.seller, share, "¡Has vendido tu objeto en la subasta!");
                self.announce(format!("Subasta » {buyer} compró {obj_name} (x{}) por {} monedas de oro.", l.amount, l.best));
            }
            None => {
                self.deliver(&l.seller, l.obj, l.amount, "Tu subasta terminó sin ofertas: te devolvemos el objeto.");
                if let Some(s) = self.id_by_name(&l.seller) {
                    self.console(s, format!("La subasta de {obj_name} terminó sin ofertas."), Tone::Info);
                }
            }
        }
        self.broadcast_lots();
    }

    /// Gold to a character: in hand if online (and it fits), else to the mailbox.
    fn pay(&mut self, name: &str, gold: u32, note: &str) {
        if gold == 0 {
            return;
        }
        if let Some(id) = self.id_by_name(name) {
            if let Some(total) = self.player(id).unwrap().inv.gold.checked_add(gold) {
                self.player_mut(id).inv.gold = total;
                self.event_send_gold(id);
                self.event_save(id);
                return self.console(id, format!("{note} (+{gold} monedas de oro)"), Tone::Good);
            }
        }
        self.mail(name, gold, 0, 0, note);
    }

    /// An object to a character: in the inventory if online and it fits, else the mailbox.
    fn deliver(&mut self, name: &str, obj: u16, amount: u32, note: &str) {
        if let Some(id) = self.id_by_name(name) {
            if self.add_item(id, InvItem { obj, amount }) {
                self.event_save(id);
                return self.console(id, note, Tone::Good);
            }
            self.console(id, "No tienes lugar: el subastador te lo guarda hasta que vuelvas a entrar con espacio.", Tone::Warn);
        }
        self.mail(name, 0, obj, amount, note);
    }

    fn mail(&self, name: &str, gold: u32, obj: u16, amount: u32, note: &str) {
        if let Err(e) = self.db.mail(name, gold, obj, amount, note) {
            warn!(?e, name, gold, obj, amount, "no se pudo guardar en el buzón");
        }
    }

    /// Hands over what was kept while the character was away (or had a full inventory).
    fn deliver_mailbox(&mut self, id: u32) {
        let name = self.name_of(id);
        let Ok(rows) = self.db.mailbox(&name) else { return };
        for m in rows {
            if self.player(id).unwrap().inv.gold.checked_add(m.gold).is_none() {
                continue;
            }
            if m.amount > 0 && m.obj != 0 && !self.add_item(id, InvItem { obj: m.obj, amount: m.amount }) {
                continue;
            }
            if m.gold > 0 {
                self.player_mut(id).inv.gold += m.gold;
                self.event_send_gold(id);
            }
            self.event_save(id);
            if let Err(e) = self.db.delete_mail(m.id) {
                warn!(?e, "no se pudo vaciar el buzón");
            }
            let extra = if m.gold > 0 { format!(" (+{} monedas de oro)", m.gold) } else { String::new() };
            self.console(id, format!("El subastador te ha dejado un mensaje: {}{extra}", m.note), Tone::Good);
        }
    }

    // ================================================================== tournaments

    /// `HandleCrearTorneo` + `IniciarTorneo` (GM).
    fn tournament_open(&mut self, id: u32, spec: TournamentSpec) {
        if !self.event_is_staff(id) {
            return self.console(id, "Solo los administradores pueden organizar torneos.", Tone::Warn);
        }
        if self.events.tournament.is_some() {
            return self.console(id, "Ya hay un torneo abierto.", Tone::Warn);
        }
        if spec.slots == 0 || spec.name.trim().is_empty() || spec.min_level > spec.max_level || spec.map == 0 || !self.ensure_map(spec.map) {
            return self.console(id, "Datos del torneo inválidos.", Tone::Warn);
        }
        let classes = if spec.classes.is_empty() { "todas".to_string() } else { spec.classes.iter().map(|c| class_name(*c)).collect::<Vec<_>>().join(", ") };
        self.announce(format!(
            "Evento> Están abiertas las inscripciones para: {}: características: Nivel entre: {}/{}. Inscriptos: 0/{}. Precio de inscripción: {} monedas de oro. Reglas: {}.",
            spec.name, spec.min_level, spec.max_level, spec.slots, spec.fee, spec.rules
        ));
        self.announce(format!("Evento> Clases participantes: {classes}. Inscríbete en la ventana de eventos (V)."));
        self.events.tournament = Some(Tournament { spec, signed: Vec::new(), started: false });
        let all = self.player_ids();
        self.send_states(all);
    }

    /// `ParticiparTorneo` with the checks the sign-up needs (level, class, slots, fee).
    fn tournament_join(&mut self, id: u32) {
        let Some(t) = &self.events.tournament else {
            return self.console(id, "No hay ningún torneo abierto.", Tone::Info);
        };
        let p = self.player(id).unwrap();
        let why = if t.started {
            Some("Las inscripciones ya están cerradas.".to_string())
        } else if p.event.in_tournament {
            Some("¡Ya estás anotado! Solo debes aguardar hasta que seas enviado a la sala de espera.".to_string())
        } else if t.signed.len() >= t.spec.slots as usize {
            Some("No quedan cupos.".to_string())
        } else if (p.level as u8) < t.spec.min_level || p.level > t.spec.max_level as u32 {
            Some(format!("El torneo es para niveles {} a {}.", t.spec.min_level, t.spec.max_level))
        } else if !t.spec.classes.is_empty() && !t.spec.classes.contains(&p.class) {
            Some("Tu clase no participa de este torneo.".to_string())
        } else if p.event.room.is_some() || p.event.request.is_some() || p.event.accepted.is_some() {
            Some("No puedes anotarte con un reto pendiente.".to_string())
        } else if p.inv.gold < t.spec.fee {
            Some(format!("Necesitas {} monedas de oro para inscribirte.", t.spec.fee))
        } else {
            None
        };
        if let Some(w) = why {
            return self.console(id, w, Tone::Warn);
        }
        let fee = t.spec.fee;
        let name = p.name.clone();
        let p = self.player_mut(id);
        p.inv.gold -= fee;
        p.event.in_tournament = true;
        self.event_send_gold(id);
        self.events.tournament.as_mut().unwrap().signed.push((id, name));
        self.console(id, "¡Ya estas anotado! Solo debes aguardar hasta que seas enviado a la sala de espera.", Tone::Good);
        let all = self.player_ids();
        self.send_states(all);
    }

    /// `BorrarIndexInTorneo`: off the list; the fee comes back while it has not started.
    fn tournament_leave(&mut self, id: u32, tell: bool) {
        let Some(t) = self.events.tournament.as_mut() else { return };
        let Some(i) = t.signed.iter().position(|(s, _)| *s == id) else {
            if tell {
                self.console(id, "No estás anotado en el torneo.", Tone::Info);
            }
            return;
        };
        t.signed.remove(i);
        let refund = if t.started { 0 } else { t.spec.fee };
        let p = self.player_mut(id);
        p.event.in_tournament = false;
        p.inv.gold = p.inv.gold.saturating_add(refund);
        self.event_send_gold(id);
        if tell {
            self.console(id, "Te has dado de baja del torneo.", Tone::Info);
        }
        let all = self.player_ids();
        self.send_states(all);
    }

    /// `ComenzarTorneoOk` (GM): everyone signed up goes to the tournament map.
    fn tournament_start(&mut self, id: u32) {
        if !self.event_is_staff(id) {
            return self.console(id, "Solo los administradores pueden organizar torneos.", Tone::Warn);
        }
        let Some(t) = self.events.tournament.as_mut().filter(|t| !t.started) else {
            return self.console(id, "No hay un torneo esperando para empezar.", Tone::Warn);
        };
        t.started = true;
        let (signed, map, x, y) = (t.signed.clone(), t.spec.map, t.spec.x as i32, t.spec.y as i32);
        for (pid, _) in &signed {
            self.teleport(*pid, map, x, y);
        }
        let names: Vec<String> = signed.into_iter().map(|(_, n)| n).collect();
        self.announce(format!("Evento> Los elegidos para participar son: {} damos inicio al evento.", names.join(", ")));
        let all = self.player_ids();
        self.send_states(all);
    }

    /// `ResetearTorneo` (GM). If it never started, the fees are returned.
    fn tournament_close(&mut self, id: u32) {
        if !self.event_is_staff(id) {
            return self.console(id, "Solo los administradores pueden organizar torneos.", Tone::Warn);
        }
        let Some(t) = self.events.tournament.take() else {
            return self.console(id, "No hay ningún torneo abierto.", Tone::Info);
        };
        for (pid, name) in &t.signed {
            if self.player(*pid).is_some_and(|p| &p.name == name) {
                let p = self.player_mut(*pid);
                p.event.in_tournament = false;
                if !t.started {
                    p.inv.gold = p.inv.gold.saturating_add(t.spec.fee);
                }
                self.event_send_gold(*pid);
            }
        }
        if t.started {
            self.events.house_gold += t.spec.fee as u64 * t.signed.len() as u64;
        }
        self.announce("Eventos> Evento Finalizado.");
        let all = self.player_ids();
        self.send_states(all);
    }
}

/// Whole duels on a world with a virtual clock: kills are dealt straight through
/// `hurt_player` (walking two bots across a ring to hit each other is not what is tested).
#[cfg(test)]
mod duel_tests {
    use super::*;
    use crate::clock::{Clock, VirtualClock};
    use crate::net::Event;
    use crate::systems::combat::Source;
    use crate::Settings;
    use ao_protocol::{C2S, PROTOCOL_VERSION};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    struct T {
        w: World,
        clock: VirtualClock,
        _rx: Vec<UnboundedReceiver<Vec<u8>>>,
    }

    const BET: u32 = 10_000;
    const START_GOLD: u32 = 50_000;

    fn world(n: u32) -> T {
        let clock = VirtualClock::new();
        let mut s = Settings::defaults();
        s.clock = Clock::Virtual(clock.clone());
        s.npcs = false;
        let mut w = World::new(s);
        let mut rx = Vec::new();
        for id in 1..=n {
            let (tx, r) = unbounded_channel();
            w.on_event(Event::Connected { id, addr: "127.0.0.1:9".parse().unwrap(), tx });
            w.on_event(Event::Frame { id, msg: C2S::Login { name: format!("Pj{id}"), version: PROTOCOL_VERSION } });
            w.player_mut(id).inv.gold = START_GOLD;
            rx.push(r);
        }
        T { w, clock, _rx: rx }
    }

    impl T {
        fn run_secs(&mut self, secs: u64) {
            let step = self.w.cfg.world.tick_ms.max(1);
            let end = self.clock.now().after_secs(secs);
            while self.clock.now() < end {
                let t = self.clock.now().after_ms(step);
                self.clock.set(t);
                self.w.tick(t);
            }
        }
        fn gold(&self, id: u32) -> u32 {
            self.w.player(id).unwrap().inv.gold
        }
        fn total(&self, ids: &[u32]) -> u64 {
            ids.iter().filter_map(|i| self.w.player(*i)).map(|p| p.inv.gold as u64).sum::<u64>() + self.w.events.house_gold
        }
        fn kill(&mut self, killer: u32, victim: u32) {
            assert!(self.w.hurt_player(victim, 1_000_000, Source::Player(killer)));
        }
        fn propose(&mut self, id: u32, players: &[u32]) {
            let players = players.iter().map(|p| format!("Pj{p}")).collect();
            self.w.on_game_event(id, EventC2S::DuelPropose { players, bet: BET, max_potions: -1 });
        }
        fn accept(&mut self, id: u32, offerer: u32) {
            self.w.on_game_event(id, EventC2S::DuelAccept { offerer: format!("Pj{offerer}") });
        }
        fn room(&self, id: u32) -> Option<usize> {
            self.w.player(id).unwrap().event.room
        }
        fn map(&self, id: u32) -> u16 {
            self.w.player(id).unwrap().map
        }
    }

    #[test]
    fn one_vs_one_best_of_three_pays_the_winner_minus_the_tax() {
        let mut t = world(2);
        let home = t.map(1);
        t.propose(1, &[2]);
        assert_eq!(t.gold(1), START_GOLD, "nothing is taken on proposing");
        t.accept(2, 1);
        assert_eq!((t.gold(1), t.gold(2)), (START_GOLD - BET, START_GOLD - BET), "both bets go to the pot at the start");
        assert!(t.room(1).is_some() && t.room(1) == t.room(2));
        assert!([324, 372, 389, 390].contains(&t.map(1)), "an official ring of Retos.dat");
        assert_eq!(t.map(1), t.map(2));
        assert!(t.w.event_keeps_items_on_death(2));
        // Countdown: held in place.
        let now = t.w.now();
        assert!(t.w.player(1).unwrap().status.immobilized_until.is_some_and(|u| u > now));
        t.run_secs(16);
        t.kill(1, 2);
        t.run_secs(1);
        let d = t.w.events.rooms[t.room(1).unwrap()].as_ref().unwrap();
        assert_eq!((d.round, d.wins_a, d.wins_b), (2, 1, 0));
        assert!(!t.w.is_dead(2), "revived for the next round");
        t.run_secs(16);
        t.kill(1, 2);
        t.run_secs(1);
        assert_eq!(t.room(1), None);
        assert_eq!(t.gold(1), START_GOLD - BET + 18_000, "pot 20 000 minus 10 %");
        assert_eq!(t.gold(2), START_GOLD - BET);
        assert_eq!(t.w.events.house_gold, 2_000);
        assert_eq!((t.map(1), t.map(2)), (home, home), "back where they were");
        assert!(!t.w.is_dead(2));
        assert_eq!(t.total(&[1, 2]), 2 * START_GOLD as u64);
    }

    #[test]
    fn two_vs_two_and_the_split_between_winners() {
        let mut t = world(4);
        // Pj1 + Pj3 vs Pj2 + Pj4.
        t.propose(1, &[2, 3, 4]);
        t.accept(2, 1);
        t.accept(3, 1);
        assert_eq!(t.room(1), None, "Pj4 is still missing");
        t.accept(4, 1);
        let room = t.room(1).expect("started");
        let d = t.w.events.rooms[room].as_ref().unwrap();
        assert_eq!(d.a.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Pj1", "Pj3"]);
        assert_eq!(d.pot, 4 * BET as u64);
        for _ in 0..2 {
            t.run_secs(16);
            t.kill(1, 2);
            t.run_secs(1);
            assert!(t.room(1).is_some(), "one rival still standing");
            t.kill(3, 4);
            t.run_secs(1);
        }
        assert_eq!(t.room(1), None);
        assert_eq!((t.gold(1), t.gold(3)), (START_GOLD - BET + 18_000, START_GOLD - BET + 18_000));
        assert_eq!((t.gold(2), t.gold(4)), (START_GOLD - BET, START_GOLD - BET));
        assert_eq!(t.total(&[1, 2, 3, 4]), 4 * START_GOLD as u64);
    }

    #[test]
    fn disconnecting_mid_duel_loses_it_and_is_saved_out_of_the_ring() {
        let mut t = world(2);
        let home = t.map(2);
        t.propose(1, &[2]);
        t.accept(2, 1);
        t.run_secs(16);
        t.kill(1, 2);
        // Dead and disconnected: the leave hook revives and returns them before the save.
        t.w.event_on_leave(2);
        assert_eq!(t.map(2), home);
        assert!(!t.w.is_dead(2));
        t.w.on_event(Event::Disconnected { id: 2 });
        assert_eq!(t.room(1), None);
        assert_eq!(t.gold(1), START_GOLD - BET + 18_000);
        assert_eq!(t.w.events.rooms.iter().filter(|r| r.is_some()).count(), 0, "the room is free");
        // Later deaths or ticks pay nothing twice.
        t.run_secs(30);
        assert_eq!(t.gold(1), START_GOLD - BET + 18_000);
    }

    #[test]
    fn a_teammate_leaving_does_not_end_a_2v2_but_the_last_one_dying_loses_the_round() {
        let mut t = world(4);
        t.propose(1, &[2, 3, 4]);
        for p in [2, 3, 4] {
            t.accept(p, 1);
        }
        t.run_secs(16);
        t.w.on_game_event(4, EventC2S::DuelQuit);
        assert_eq!(t.room(4), None);
        assert!(t.room(2).is_some(), "Pj2 fights on alone");
        t.kill(1, 2);
        t.run_secs(1);
        let d = t.w.events.rooms[t.room(1).unwrap()].as_ref().unwrap();
        assert_eq!(d.wins_a, 1);
        t.run_secs(16);
        t.kill(3, 2);
        t.run_secs(1);
        // The winners share the whole pot, Pj4's bet included.
        assert_eq!(t.gold(1), START_GOLD - BET + 18_000);
        assert_eq!(t.total(&[1, 2, 3, 4]), 4 * START_GOLD as u64);
    }

    #[test]
    fn time_out_is_a_draw_and_the_pot_is_shared() {
        let mut t = world(2);
        t.propose(1, &[2]);
        t.accept(2, 1);
        t.run_secs(601);
        assert_eq!(t.room(1), None);
        assert_eq!((t.gold(1), t.gold(2)), (START_GOLD - BET + 9_000, START_GOLD - BET + 9_000));
        assert_eq!(t.total(&[1, 2]), 2 * START_GOLD as u64);
    }

    #[test]
    fn spending_the_bet_before_the_start_cancels_it_and_nobody_pays() {
        let mut t = world(4);
        t.propose(1, &[2, 3, 4]);
        t.accept(2, 1);
        // Pj2 gets rid of the gold after accepting (drops it, banks it...).
        t.w.player_mut(2).inv.gold = BET - 1;
        t.accept(3, 1);
        t.accept(4, 1);
        assert!([1, 2, 3, 4].iter().all(|p| t.room(*p).is_none()));
        assert!(t.w.player(1).unwrap().event.request.is_none(), "cancelled");
        assert_eq!((t.gold(1), t.gold(3), t.gold(4)), (START_GOLD, START_GOLD, START_GOLD));
        assert!([2, 3, 4].iter().all(|p| t.w.player(*p).unwrap().event.accepted.is_none()));
    }

    #[test]
    fn open_lots_survive_a_restart_and_close_by_mail() {
        let mut t = world(1);
        let row = LotRow { id: 41, seller: "Lejano".into(), obj: RED_POTION, amount: 7, base_price: 900, best: 1200, bidder: Some("Ausente".into()), secs_left: 30 };
        t.w.db.save_lot(&row).unwrap();
        t.w.events.lots_loaded = false;
        t.w.events.lots.clear();
        t.run_secs(1);
        assert_eq!(t.w.events.lots.len(), 1, "read back from the database");
        assert_eq!(t.w.events.next_lot, 41);
        t.run_secs(31);
        assert!(t.w.events.lots.is_empty());
        assert!(t.w.db.load_lots().unwrap().is_empty());
        let buyer = t.w.db.mailbox("Ausente").unwrap();
        assert_eq!((buyer.len(), buyer[0].obj, buyer[0].amount), (1, RED_POTION, 7));
        let seller = t.w.db.mailbox("Lejano").unwrap();
        assert_eq!(seller[0].gold, 1140, "1200 minus 5 %");
        assert_eq!(t.w.events.house_gold, 60);
    }

    #[test]
    fn a_full_house_queues_the_duel_until_a_room_is_freed() {
        let mut t = world(4);
        t.w.duel_config();
        // Every room but one busy: fill the remaining with Pj1 vs Pj2, then Pj3 vs Pj4 waits.
        for (r, slot) in t.w.events.rooms.iter_mut().enumerate().skip(1) {
            *slot = Some(Duel { room: r, a: vec![], b: vec![], bet: 0, pot: 0, round: 1, wins_a: 0, wins_b: 0, fight_at: GameTime::ZERO, ends_at: GameTime(u64::MAX), forfeit: None });
        }
        t.propose(1, &[2]);
        t.accept(2, 1);
        assert_eq!(t.room(1), Some(0));
        t.propose(3, &[4]);
        t.accept(4, 3);
        assert!(t.w.player(3).unwrap().event.request.as_ref().is_some_and(|r| r.queued));
        assert_eq!(t.gold(3), START_GOLD, "nothing taken while queued");
        t.w.on_game_event(2, EventC2S::DuelQuit);
        assert_eq!(t.room(3), Some(0), "takes the freed room");
        assert_eq!(t.gold(3), START_GOLD - BET);
    }
}

#[cfg(test)]
mod tests {
    use super::rules::*;

    #[test]
    fn auction_minimum_bids() {
        assert_eq!(min_bid(5000, 0), 5000, "the base price is enforced");
        assert_eq!(min_bid(500, 1000), 1100, "at least 100 more");
        assert_eq!(min_bid(5000, 5000), 5250, "5 % above 2000");
        assert_eq!(min_bid(5000, 10_000), 10_500, "5 % above 2000");
        assert_eq!(seller_share(10_000), 9_500);
        assert_eq!(seller_share(19), 19, "5 % of 19 rounds down to 0");
    }

    #[test]
    fn duel_pot_and_rounds() {
        assert_eq!(pot_after_tax(20_000, 0.10), 18_000);
        assert_eq!(pot_after_tax(40_001, 0.10), 36_000);
        assert!(!duel_over(1, 1, 0));
        assert!(duel_over(2, 2, 0));
        assert!(duel_over(3, 1, 1));
        assert_eq!(dat_num(Some(&"5\t\t\t# N vs N".to_string())), 5.0);
        assert_eq!(dat_num(Some(&"0.10 \t# impuesto".to_string())), 0.10);
        assert_eq!(dat_num(None), 0.0);
    }
}
