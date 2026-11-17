//! Map chat with flood control and sanitising.
use crate::security::sanitize_chat;
use crate::world::World;
use ao_protocol::{Tone, S2C};
use ao_rules::MAX_CHAT;

impl World {
    /// P2 chat channels (whisper, party, guild, global...). OWNER: Sonnet 2.
    pub(crate) fn on_channel(&mut self, id: u32, msg: ao_protocol::ChannelC2S) {
        let _ = id;
        match msg {}
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
            _ => false,
        }
    }
}
