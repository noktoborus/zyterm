//! `zyt.net`: ports a script listens on, and the connections they take.
//!
//! A script may open a port of this machine and carry the line to whoever
//! connects — a telnet client, another program. Every socket goes into
//! [`SocketRegistry`], and the end of the run closes what is left there, so a
//! script that failed or was stopped leaves no port taken and no client
//! talking to nothing.
//!
//! Every wait is made of slices, as a read of the line is, and the ask to stop
//! is read between two of them.
//!
//! [`SocketRegistry`]: crate::sockets::SocketRegistry

use super::{Context, external};
use crate::error::ScriptError;
use crate::sockets::{Held, SocketId, SocketRegistry};
use mlua::{Lua, Table, UserData, UserDataMethods};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long one wait inside a longer one lasts.
const SLICE: Duration = Duration::from_millis(50);

/// How often a port is asked whether somebody connected.
const POLL: Duration = Duration::from_millis(10);

/// How much of a connection is read at once.
const CHUNK: usize = 16 * 1024;

/// Builds the table.
pub(crate) fn table(lua: &Lua, context: &Arc<Context>) -> mlua::Result<Table> {
    let net = lua.create_table()?;

    net.set(
        "listen",
        lua.create_function({
            let context = context.clone();
            move |_, how: Table| {
                context.check()?;
                let host: Option<String> = how.get("host")?;
                let host = host
                    .filter(|host| !host.trim().is_empty())
                    .unwrap_or_else(|| String::from("127.0.0.1"));
                let port: u16 = how.get("port")?;
                let address = joined(host.trim(), port);

                let listener = TcpListener::bind(&address)
                    .map_err(|source| failed(format!("cannot listen on {address}"), source))?;
                listener
                    .set_nonblocking(true)
                    .map_err(|source| failed(format!("cannot listen on {address}"), source))?;
                let bound = listener
                    .local_addr()
                    .map(|address| address.to_string())
                    .unwrap_or(address);

                let id = registry(&context)?.adopt(Held::Listener(Arc::new(listener)));
                log::debug!("{} listens on {bound}", context.name);
                Ok(Listener {
                    socket: Socket::new(id, &context),
                    address: bound,
                })
            }
        })?,
    )?;

    Ok(net)
}

/// What a socket handle of the script carries: its number in the list, and
/// the list itself.
struct Socket {
    id: SocketId,
    sockets: Arc<Mutex<SocketRegistry>>,
    context: Arc<Context>,
}

impl Socket {
    fn new(id: SocketId, context: &Arc<Context>) -> Self {
        Self {
            id,
            sockets: context.sockets.clone(),
            context: context.clone(),
        }
    }

    /// The socket, while it is open.
    fn held(&self) -> Option<Held> {
        self.sockets.lock().ok().and_then(|list| list.get(self.id))
    }

    fn close(&self) {
        if let Ok(mut list) = self.sockets.lock() {
            list.close(self.id);
        }
    }
}

impl Drop for Socket {
    /// A handle the script let go of closes its socket: nothing can reach it
    /// any more.
    fn drop(&mut self) {
        self.close();
    }
}

/// A port a script listens on.
struct Listener {
    socket: Socket,
    address: String,
}

impl Listener {
    fn listener(&self) -> mlua::Result<Arc<TcpListener>> {
        match self.socket.held() {
            Some(Held::Listener(listener)) => Ok(listener),
            _ => Err(failed(
                format!("{} is closed", self.address),
                ErrorKind::NotConnected.into(),
            )),
        }
    }

    /// Waits for somebody to connect, and answers with nothing when nobody
    /// did in time.
    fn accept(&self, timeout: Duration) -> mlua::Result<Option<Connection>> {
        let context = &self.socket.context;
        let deadline = Instant::now() + timeout;
        loop {
            context.check()?;
            let listener = self.listener()?;
            match listener.accept() {
                Ok((stream, peer)) => {
                    let peer = peer.to_string();
                    let ready = stream
                        .set_nonblocking(false)
                        .and_then(|()| stream.set_nodelay(true));
                    if let Err(source) = ready {
                        return Err(failed(format!("cannot take {peer}"), source));
                    }
                    let id = registry(context)?.adopt(Held::Stream(Arc::new(stream)));
                    log::debug!("{} took {peer} on {}", context.name, self.address);
                    return Ok(Some(Connection {
                        socket: Socket::new(id, context),
                        peer,
                    }));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(source) => {
                    return Err(failed(format!("cannot accept on {}", self.address), source));
                }
            }
            drop(listener);

            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            context.cancel.sleep(POLL.min(left)).map_err(external)?;
        }
    }
}

impl UserData for Listener {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("accept", |_, this, timeout: Option<u64>| {
            this.accept(Duration::from_millis(timeout.unwrap_or(0)))
        });
        methods.add_method("address", |_, this, ()| Ok(this.address.clone()));
        methods.add_method("is_open", |_, this, ()| Ok(this.socket.held().is_some()));
        methods.add_method("close", |_, this, ()| {
            this.socket.close();
            Ok(())
        });
    }
}

/// One connection a port took.
struct Connection {
    socket: Socket,
    peer: String,
}

/// What a read of a connection came to.
enum Came {
    Bytes(Vec<u8>),
    Nothing,
    Closed,
}

impl Connection {
    fn stream(&self) -> Option<Arc<TcpStream>> {
        match self.socket.held() {
            Some(Held::Stream(stream)) => Some(stream),
            _ => None,
        }
    }

    /// Reads for as long as it is given, in slices.
    fn read(&self, timeout: Duration) -> mlua::Result<Came> {
        let context = &self.socket.context;
        let deadline = Instant::now() + timeout;
        let mut buffer = vec![0u8; CHUNK];
        loop {
            context.check()?;
            let Some(stream) = self.stream() else {
                return Ok(Came::Closed);
            };
            let left = deadline.saturating_duration_since(Instant::now());
            let _ = stream.set_read_timeout(Some(SLICE.min(left).max(Duration::from_millis(1))));
            match (&*stream).read(&mut buffer) {
                Ok(0) => {
                    self.socket.close();
                    return Ok(Came::Closed);
                }
                Ok(count) => {
                    buffer.truncate(count);
                    return Ok(Came::Bytes(buffer));
                }
                Err(error) if waiting(&error) => {}
                Err(error) => {
                    log::debug!("{} lost {}: {error}", context.name, self.peer);
                    self.socket.close();
                    return Ok(Came::Closed);
                }
            }
            if Instant::now() >= deadline {
                return Ok(Came::Nothing);
            }
        }
    }

    /// Writes all of it, or says the connection is gone.
    fn write(&self, data: &[u8]) -> mlua::Result<bool> {
        let context = &self.socket.context;
        let mut left = data;
        while !left.is_empty() {
            context.check_writing()?;
            let Some(stream) = self.stream() else {
                return Ok(false);
            };
            let _ = stream.set_write_timeout(Some(SLICE));
            match (&*stream).write(left) {
                Ok(0) => {
                    self.socket.close();
                    return Ok(false);
                }
                Ok(count) => left = &left[count..],
                Err(error) if waiting(&error) => {}
                Err(error) => {
                    log::debug!("{} lost {}: {error}", context.name, self.peer);
                    self.socket.close();
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

impl UserData for Connection {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("read", |lua, this, timeout: Option<u64>| {
            let came = this.read(Duration::from_millis(timeout.unwrap_or(0)))?;
            let answer = match came {
                Came::Bytes(bytes) => (
                    mlua::Value::String(lua.create_string(&bytes)?),
                    mlua::Value::Nil,
                ),
                Came::Nothing => (mlua::Value::Nil, mlua::Value::Nil),
                Came::Closed => (
                    mlua::Value::Nil,
                    mlua::Value::String(lua.create_string("closed")?),
                ),
            };
            Ok(answer)
        });
        methods.add_method("write", |lua, this, data: mlua::LuaString| {
            if this.write(&data.as_bytes())? {
                Ok((mlua::Value::Boolean(true), mlua::Value::Nil))
            } else {
                Ok((
                    mlua::Value::Nil,
                    mlua::Value::String(lua.create_string("closed")?),
                ))
            }
        });
        methods.add_method("peer", |_, this, ()| Ok(this.peer.clone()));
        methods.add_method("is_open", |_, this, ()| Ok(this.stream().is_some()));
        methods.add_method("close", |_, this, ()| {
            this.socket.close();
            Ok(())
        });
    }
}

/// The socket a handle of the script stands for, for a wait: nothing when the
/// value is no socket of `zyt.net`, and an inner nothing when it is closed.
pub(crate) fn held_of(value: &mlua::AnyUserData) -> Option<Option<Held>> {
    if let Ok(listener) = value.borrow::<Listener>() {
        return Some(listener.socket.held());
    }
    if let Ok(connection) = value.borrow::<Connection>() {
        return Some(connection.socket.held());
    }
    None
}

/// The list of sockets, or the error of a run whose list is gone.
fn registry(context: &Context) -> mlua::Result<std::sync::MutexGuard<'_, SocketRegistry>> {
    context.sockets.lock().map_err(|_| {
        failed(
            String::from("the sockets of the script"),
            ErrorKind::Other.into(),
        )
    })
}

/// A host and a port as one address, an IPv6 host in brackets.
fn joined(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// True for the errors a wait with a timeout ends in.
fn waiting(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    )
}

fn failed(what: String, source: std::io::Error) -> mlua::Error {
    external(ScriptError::Net { what, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_of_six_is_put_in_brackets() {
        assert_eq!(joined("127.0.0.1", 2323), "127.0.0.1:2323");
        assert_eq!(joined("::1", 2323), "[::1]:2323");
        assert_eq!(joined("[::1]", 2323), "[::1]:2323");
        assert_eq!(joined("localhost", 23), "localhost:23");
    }
}
