//! Every socket one script opened, and how they are all closed.
//!
//! A script that ended must not leave a port listening or a client connected:
//! the next run would find the port taken, and the client would talk to
//! nothing. Nothing it can call opens a socket on its own — `zyt.net` is the
//! only way — so every socket is in this list, and the end of the run closes
//! all of it, whether the script closed them, failed or was given up on.
//!
//! The list holds the socket and a handle of the script holds its number: a
//! socket closed here is gone for the handle as well, so a script that kept a
//! handle past the end of what it opened is told so rather than writing into
//! a connection nobody reads.

use std::collections::BTreeMap;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;

/// Number of a socket of this script, handed out in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SocketId(pub u64);

/// One socket, shared with the call that is using it at the moment.
#[derive(Debug, Clone)]
pub(crate) enum Held {
    /// A port waiting for connections.
    Listener(Arc<TcpListener>),
    /// One connection.
    Stream(Arc<TcpStream>),
}

/// The sockets of one script.
#[derive(Debug, Default)]
pub(crate) struct SocketRegistry {
    next: u64,
    held: BTreeMap<SocketId, Held>,
}

impl SocketRegistry {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes a socket into the list and answers with its number.
    pub fn adopt(&mut self, socket: Held) -> SocketId {
        let id = SocketId(self.next.max(1));
        self.next = id.0 + 1;
        self.held.insert(id, socket);
        id
    }

    /// The socket of that number, while it is open.
    ///
    /// It is handed out shared, so the call using it holds no lock while it
    /// waits; a socket closed meanwhile is shut under that call, which then
    /// ends at once.
    pub fn get(&self, id: SocketId) -> Option<Held> {
        self.held.get(&id).cloned()
    }

    /// Closes one of them.
    pub fn close(&mut self, id: SocketId) {
        if let Some(socket) = self.held.remove(&id) {
            shut(&socket);
        }
    }

    /// Closes every one of them, the newest first: a connection goes before
    /// the port it came through.
    pub fn close_all(&mut self) {
        while let Some((_, socket)) = self.held.pop_last() {
            shut(&socket);
        }
    }

    /// How many are open.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.held.len()
    }
}

impl Drop for SocketRegistry {
    fn drop(&mut self) {
        self.close_all();
    }
}

/// Shuts a socket for whoever still holds it.
///
/// A connection is shut both ways, so a call reading or writing it at this
/// moment returns instead of waiting out its slice. A port has no such call;
/// it is closed when the last call using it lets go, which is one slice of
/// waiting at most.
fn shut(socket: &Held) {
    if let Held::Stream(stream) = socket {
        let _ = stream.shutdown(Shutdown::Both);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn closing_all_shuts_a_connection_for_the_other_end() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let mut client = TcpStream::connect(address).expect("connect");
        let (accepted, _) = listener.accept().expect("accept");

        let mut registry = SocketRegistry::new();
        registry.adopt(Held::Listener(Arc::new(listener)));
        let id = registry.adopt(Held::Stream(Arc::new(accepted)));
        assert_eq!(registry.len(), 2);

        registry.close_all();
        assert_eq!(registry.len(), 0);
        assert!(registry.get(id).is_none(), "and the handle finds nothing");

        let mut read = [0u8; 8];
        assert_eq!(
            client.read(&mut read).expect("read"),
            0,
            "the far end sees the end"
        );
        drop(client);
        // The connection ends once the far end has said so too, which the
        // kernel takes in a moment after the client lets go.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while TcpListener::bind(address).is_err() {
            assert!(
                std::time::Instant::now() < deadline,
                "the port is free for the next run"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
