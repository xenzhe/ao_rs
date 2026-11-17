//! Non-blocking TCP connection polled once per frame (no threads needed).
use ao_protocol::{encode, FrameReader, C2S, MAX_SERVER_FRAME, S2C};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub struct Net {
    sock: TcpStream,
    reader: FrameReader,
    out: Vec<u8>,
    alive: bool,
}

impl Net {
    pub fn connect(addr: &str) -> std::io::Result<Net> {
        let sa = addr.to_socket_addrs()?.next().ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "direccion invalida"))?;
        let sock = TcpStream::connect_timeout(&sa, Duration::from_secs(2))?;
        sock.set_nodelay(true)?;
        sock.set_nonblocking(true)?;
        Ok(Net { sock, reader: FrameReader::default(), out: Vec::new(), alive: true })
    }

    pub fn send(&mut self, msg: &C2S) {
        self.out.extend_from_slice(&encode(msg));
        self.flush();
    }

    fn flush(&mut self) {
        while !self.out.is_empty() {
            match self.sock.write(&self.out) {
                Ok(0) => {
                    self.alive = false;
                    return;
                }
                Ok(n) => {
                    self.out.drain(..n);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                Err(_) => {
                    self.alive = false;
                    return;
                }
            }
        }
    }

    /// Returns every complete message received so far and whether the link is still up.
    pub fn poll(&mut self) -> (Vec<S2C>, bool) {
        self.flush();
        let mut buf = [0u8; 16384];
        loop {
            match self.sock.read(&mut buf) {
                Ok(0) => {
                    self.alive = false;
                    break;
                }
                Ok(n) => self.reader.push(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.alive = false;
                    break;
                }
            }
        }
        let mut msgs = Vec::new();
        while let Some(m) = self.reader.next::<S2C>(MAX_SERVER_FRAME) {
            match m {
                Ok(m) => msgs.push(m),
                Err(_) => {
                    self.alive = false;
                    break;
                }
            }
        }
        (msgs, self.alive)
    }
}
