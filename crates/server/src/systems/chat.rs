//! Map chat with flood control and sanitising.
use crate::security::sanitize_chat;
use crate::world::World;
use ao_protocol::{Tone, S2C};
use ao_rules::MAX_CHAT;

impl World {
    pub(crate) fn on_say(&mut self, id: u32, text: String) {
        let Some(clean) = sanitize_chat(&text, MAX_CHAT) else { return };
        let now = self.now();
        let p = self.player_mut(id);
        if !p.chat.allow(now) {
            return self.console(id, "No hagas flood.", Tone::Warn);
        }
        let (name, map) = (p.name.clone(), p.map);
        self.broadcast(map, &S2C::Console { text: format!("{name}: {clean}"), color: Tone::Chat }, None);
    }
}
