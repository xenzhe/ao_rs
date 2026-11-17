//! Gameplay systems, one file each, all as `impl World` blocks. A new feature (inventory,
//! trade, guilds...) is a new file here plus its messages in `ao-protocol`.
mod account;
mod chat;
mod combat;
mod magic;
mod movement;
mod npc_ai;
mod regen;
mod session;

use crate::world::World;
use ao_protocol::C2S;

impl World {
    /// Routes a decoded client message to its system.
    pub fn handle(&mut self, id: u32, msg: C2S) {
        let logged = self.player(id).is_some();
        match msg {
            C2S::CreateAccount { .. } | C2S::AccountLogin { .. } | C2S::CreateCharacter { .. } | C2S::DeleteCharacter { .. } | C2S::PlayCharacter { .. } => {
                self.on_account(id, msg)
            }
            C2S::Login { name, version } => self.on_login(id, name, version),
            _ if !logged => self.kick(id, "paquete antes del login"),
            C2S::Walk { dir, seq } => self.on_walk(id, dir, seq),
            C2S::Cast { spell, tx, ty, seq } => self.on_cast(id, spell, tx, ty, seq),
            C2S::Say { text } => self.on_say(id, text),
        }
    }
}
