//! Map chat with flood control and sanitising.
use crate::security::sanitize_chat;
use crate::world::World;
use ao_protocol::social::channel::ChannelKind;
use ao_protocol::{ChannelC2S, ChannelS2C, Tone, S2C};
use ao_rules::magic::{VIEW_RANGE_X, VIEW_RANGE_Y};
use ao_rules::MAX_CHAT;

/// `ChatGlobal` (Configuracion.ini; code default 1). A server that wants it off flips this.
pub const GLOBAL_ENABLED: bool = true;
/// `ChatGlobalCosto`: gold charged per global line (Configuracion.ini default 2500).
pub const GLOBAL_COST: u32 = 2500;
/// `IntervaloMensajeGlobal` (intervalos.ini): minimum time between global lines.
pub const GLOBAL_INTERVAL_MS: u64 = 5000;
/// Key of the global-chat timer in `inv.cooldowns` (obj.dat CD types are >= 0, -1 is the
/// use timer): avoids touching `Player` for one timestamp.
const GLOBAL_TIMER: i32 = -2;

impl World {
    /// P2 chat channels (whisper, yell, party, guild, global). OWNER: Sonnet 2.
    /// Every line goes through the same flood limiter as map chat (`Player.chat`).
    pub(crate) fn on_channel(&mut self, id: u32, msg: ChannelC2S) {
        let (kind, text) = match &msg {
            ChannelC2S::Whisper { text, .. } => (ChannelKind::Whisper, text),
            ChannelC2S::Yell { text } => (ChannelKind::Yell, text),
            ChannelC2S::Party { text } => (ChannelKind::Party, text),
            ChannelC2S::Guild { text } => (ChannelKind::Guild, text),
            ChannelC2S::Global { text } => (ChannelKind::Global, text),
        };
        // Empty / control-only lines are dropped silently (VB: `LenB(chat) <> 0`).
        let Some(clean) = sanitize_chat(text, MAX_CHAT) else { return };
        let now = self.now();
        let Some(p) = self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()) else { return };
        if !p.chat.allow(now) {
            return self.console(id, "No hagas flood.", Tone::Warn);
        }
        let name = p.name.clone();
        let line = |target: &str| S2C::Channel(ChannelS2C::Message { channel: kind, author: name.clone(), target: target.to_string(), text: clean.clone() });
        match msg {
            ChannelC2S::Whisper { to, .. } => self.channel_whisper(id, to.trim(), &line),
            ChannelC2S::Yell { .. } => self.channel_yell(id, &line),
            ChannelC2S::Party { .. } => {
                let members = self.party_members_online(id);
                self.channel_group(id, members, "No estás en un grupo.", &line);
            }
            ChannelC2S::Guild { .. } => {
                let members = self.guild_members_online(id);
                self.channel_group(id, members, "No perteneces a ningún clan.", &line);
            }
            ChannelC2S::Global { .. } => self.channel_global(id, &line),
        }
    }

    /// HandleWhisper: only to an online, living player in view of the sender; the sender
    /// must be alive too. Both get the line.
    fn channel_whisper(&mut self, id: u32, to: &str, line: &dyn Fn(&str) -> S2C) {
        if self.is_dead(id) {
            return self.console(id, "No puedes susurrar estando muerto.", Tone::Warn);
        }
        let Some(target) = self.player_ids().into_iter().find(|t| self.player(*t).is_some_and(|p| p.name.eq_ignore_ascii_case(to))) else {
            return self.console(id, "El usuario esta muy lejos o desconectado.", Tone::Info);
        };
        if target == id {
            return self.console(id, "No puedes susurrarte a ti mismo.", Tone::Warn);
        }
        if !self.in_view(id, target) {
            return self.console(id, "El usuario esta muy lejos o desconectado.", Tone::Info);
        }
        if self.is_dead(target) {
            return self.console(id, "No puedes susurrar a un muerto.", Tone::Warn);
        }
        let target_name = self.player(target).map(|p| p.name.clone()).unwrap_or_default();
        let msg = line(&target_name);
        self.send(id, &msg);
        self.send(target, &msg);
    }

    /// HandleYell: dead players cannot yell; everyone alive in view hears it.
    fn channel_yell(&mut self, id: u32, line: &dyn Fn(&str) -> S2C) {
        if self.is_dead(id) {
            return self.console(id, "¡¡Estás muerto!!", Tone::Info);
        }
        let msg = line("");
        for pid in self.player_ids() {
            if (pid == id || self.in_view(id, pid)) && !self.is_dead(pid) {
                self.send(pid, &msg);
            }
        }
    }

    /// Party / guild: to the members the owning system reports. If the sender is not among
    /// them (not in a group, or the hook says nothing) they are told so and nothing is sent.
    fn channel_group(&mut self, id: u32, members: Vec<u32>, not_member: &str, line: &dyn Fn(&str) -> S2C) {
        if !members.contains(&id) {
            return self.console(id, not_member, Tone::Warn);
        }
        let msg = line("");
        for m in members {
            if self.player(m).is_some() {
                self.send(m, &msg);
            }
        }
    }

    /// HandleGlobalMessage: interval between lines, enough gold (charged), then everyone.
    fn channel_global(&mut self, id: u32, line: &dyn Fn(&str) -> S2C) {
        if !GLOBAL_ENABLED {
            return self.console(id, "El global se encuentra Desactivado.", Tone::Warn);
        }
        let now = self.now();
        let p = self.player_mut(id);
        if p.inv.cooldowns.get(&GLOBAL_TIMER).is_some_and(|t| now < *t) {
            return self.console(id, "No puedes escribir mensajes globales tan rápido.", Tone::Warn);
        }
        // VB stamps the timer before checking the gold.
        p.inv.cooldowns.insert(GLOBAL_TIMER, now.after_ms(GLOBAL_INTERVAL_MS));
        if p.inv.gold < GLOBAL_COST {
            return self.console(id, format!("Necesitas {GLOBAL_COST} monedas de oro para utilizar el chat global."), Tone::Warn);
        }
        p.inv.gold -= GLOBAL_COST;
        self.send_inventory_vitals(id);
        self.broadcast_all(&line(""), None);
    }

    /// `EstaPCarea`: same map and inside the visible window.
    fn in_view(&self, a: u32, b: u32) -> bool {
        match (self.player(a), self.player(b)) {
            (Some(a), Some(b)) => a.map == b.map && (a.x - b.x).abs() <= VIEW_RANGE_X && (a.y - b.y).abs() <= VIEW_RANGE_Y,
            _ => false,
        }
    }

    pub(crate) fn on_say(&mut self, id: u32, text: String) {
        let Some(clean) = sanitize_chat(&text, MAX_CHAT) else { return };
        if clean.starts_with('/') && self.dev_command(id, &clean) {
            return;
        }
        let now = self.now();
        let p = self.player_mut(id);
        if !p.chat.allow(now) {
            return self.console(id, "No hagas flood.", Tone::Warn);
        }
        let (name, map) = (p.name.clone(), p.map);
        self.broadcast(map, &S2C::Console { text: format!("{name}: {clean}"), color: Tone::Chat }, None);
    }

    /// Development commands, only with `dev_login` (never on a real server). Returns true if
    /// the line was a command (it is then not said aloud).
    fn dev_command(&mut self, id: u32, line: &str) -> bool {
        if !self.cfg.dev_login {
            return false;
        }
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("/dar") => {
                let obj = parts.next().and_then(|s| s.parse().ok());
                let amount = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                match obj {
                    Some(obj) if self.dev_give(id, obj, amount) => self.console(id, format!("Recibes {amount} x objeto {obj}."), Tone::Good),
                    _ => self.console(id, "Uso: /dar <objeto> [cantidad] (objeto de obj.dat).", Tone::Warn),
                }
                true
            }
            Some("/ir") => {
                let n: Vec<i32> = parts.filter_map(|s| s.parse().ok()).collect();
                match n[..] {
                    [map, x, y] if map > 0 && (1..=100).contains(&x) && (1..=100).contains(&y) => self.teleport(id, map as u16, x, y),
                    _ => self.console(id, "Uso: /ir <mapa> <x> <y>.", Tone::Warn),
                }
                true
            }
            _ => false,
        }
    }
}

/// In-crate tests: the party / guild hooks are empty until their owners land, so the
/// with-members path is driven through `channel_group` with an explicit member list.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Event;
    use crate::Settings;
    use ao_protocol::{FrameReader, C2S, PROTOCOL_VERSION};
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    fn login(w: &mut World, id: u32, name: &str) -> UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = unbounded_channel();
        w.on_event(Event::Connected { id, addr: "127.0.0.1:9".parse().unwrap(), tx });
        w.on_event(Event::Frame { id, msg: C2S::Login { name: name.into(), version: PROTOCOL_VERSION } });
        rx
    }

    fn got(rx: &mut UnboundedReceiver<Vec<u8>>) -> Vec<S2C> {
        let mut r = FrameReader::default();
        while let Ok(b) = rx.try_recv() {
            r.push(&b);
        }
        let mut out = Vec::new();
        while let Some(Ok(m)) = r.next::<S2C>(1 << 20) {
            out.push(m);
        }
        out
    }

    fn chan_lines(msgs: &[S2C]) -> usize {
        msgs.iter().filter(|m| matches!(m, S2C::Channel(ChannelS2C::Message { .. }))).count()
    }

    #[test]
    fn group_line_goes_to_members_only_and_requires_membership() {
        let mut w = World::new(Settings::defaults());
        let mut rx: Vec<_> = (1..=3).map(|i| login(&mut w, i, &format!("Pj{i}"))).collect();
        rx.iter_mut().for_each(|r| drop(got(r)));
        let line = |_: &str| S2C::Channel(ChannelS2C::Message { channel: ChannelKind::Party, author: "Pj1".into(), target: String::new(), text: "hi".into() });
        // Members 1 and 2 (3 is outside the group).
        w.channel_group(1, vec![1, 2], "no", &line);
        assert_eq!([chan_lines(&got(&mut rx[0])), chan_lines(&got(&mut rx[1])), chan_lines(&got(&mut rx[2]))], [1, 1, 0]);
        // A sender missing from the list (not in the group) is refused, nobody hears.
        w.channel_group(3, vec![1, 2], "No estás en un grupo.", &line);
        assert_eq!([chan_lines(&got(&mut rx[0])), chan_lines(&got(&mut rx[1]))], [0, 0]);
        assert!(got(&mut rx[2]).iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains("No estás en un grupo"))));
    }

    #[test]
    fn dead_players_cannot_whisper_or_yell_nor_be_whispered() {
        let mut w = World::new(Settings::defaults());
        let mut rx: Vec<_> = (1..=2).map(|i| login(&mut w, i, &format!("Pj{i}"))).collect();
        rx.iter_mut().for_each(|r| drop(got(r)));
        w.player_mut(1).combat.dead = true;
        w.on_channel(1, ChannelC2S::Yell { text: "x".into() });
        w.on_channel(1, ChannelC2S::Whisper { to: "Pj2".into(), text: "x".into() });
        w.player_mut(1).combat.dead = false;
        w.player_mut(2).combat.dead = true;
        w.on_channel(1, ChannelC2S::Whisper { to: "Pj2".into(), text: "x".into() });
        assert_eq!(chan_lines(&got(&mut rx[1])), 0, "dead hear nothing");
        let a = got(&mut rx[0]);
        assert_eq!(chan_lines(&a), 0);
        assert!(a.iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains("estando muerto"))));
        assert!(a.iter().any(|m| matches!(m, S2C::Console { text, .. } if text.contains("a un muerto"))));
    }
}
