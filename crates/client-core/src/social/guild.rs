//! What the client knows about guild (filled from `S2C::Guild` only). OWNER: Opus A.
use ao_protocol::social::guild::GuildInfo;
use ao_protocol::GuildS2C;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct GuildState {
    /// The own guild, if any.
    pub info: Option<GuildInfo>,
    /// Last guild list received: (name, alignment).
    pub list: Vec<(String, u8)>,
    /// Guild tag of each entity id (shown as `<clan>` under the name).
    pub tags: HashMap<u32, String>,
}

impl GuildState {
    pub fn apply(&mut self, msg: &GuildS2C) {
        match msg {
            GuildS2C::Info(info) => self.info = Some((**info).clone()),
            GuildS2C::NoGuild => self.info = None,
            GuildS2C::List(l) => self.list = l.clone(),
            GuildS2C::Tags(t) => self.tags = t.iter().cloned().collect(),
            GuildS2C::Tag { id, tag: Some(t) } => {
                self.tags.insert(*id, t.clone());
            }
            GuildS2C::Tag { id, tag: None } => {
                self.tags.remove(id);
            }
        }
    }

    pub fn tag_of(&self, id: u32) -> Option<&str> {
        self.tags.get(&id).map(String::as_str)
    }
}
