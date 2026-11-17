//! Test-only instrumentation (`Settings::probe`, `None` in production): per connection,
//! keyed by the client's socket address, how many frames the world has consumed and how
//! many bytes it has queued for that client. A test client compares them with what it
//! sent and received to know the server is done with it (a barrier, not a sleep).
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;

#[derive(Default, Clone, Copy, Debug)]
pub struct ConnProbe {
    /// Frames the world has handled (or ignored) for this connection.
    pub frames_in: u64,
    /// Bytes the world has queued to this connection.
    pub bytes_out: u64,
    /// The world saw the connection end: nothing more will be consumed or sent.
    pub closed: bool,
}

#[derive(Default, Debug)]
pub struct Probe {
    conns: Mutex<HashMap<SocketAddr, ConnProbe>>,
}

impl Probe {
    pub fn get(&self, addr: SocketAddr) -> Option<ConnProbe> {
        self.conns.lock().unwrap().get(&addr).copied()
    }

    pub(crate) fn update(&self, addr: SocketAddr, f: impl FnOnce(&mut ConnProbe)) {
        f(self.conns.lock().unwrap().entry(addr).or_default());
    }
}
