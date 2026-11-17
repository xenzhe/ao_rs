//! Accounts with several characters, persisted in SQLite. (P0: in progress.)
use crate::world::World;
use ao_protocol::{C2S, S2C};

impl World {
    pub(crate) fn on_account(&mut self, id: u32, _msg: C2S) {
        self.send(id, &S2C::AccountResult { ok: false, message: "Las cuentas todavia no estan disponibles.".into() });
    }
}
