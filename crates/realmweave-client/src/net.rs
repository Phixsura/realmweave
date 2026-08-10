//! WebSocket transport on a background thread, bridged to the Bevy world by
//! crossbeam channels. No Bevy types here.

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use realmweave_protocol::{decode, encode, ClientMessage, Envelope, ServerMessage};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

pub enum NetEvent {
    Connected,
    Message(ServerMessage),
    Disconnected(String),
}

pub struct NetHandle {
    pub tx: Sender<ClientMessage>,
    pub rx: Receiver<NetEvent>,
    alive: Arc<AtomicBool>,
}

impl Drop for NetHandle {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

impl NetHandle {
    pub fn send(&self, msg: ClientMessage) {
        let _ = self.tx.send(msg);
    }
}

/// Connect to `ws://{addr}/ws` on a background thread.
pub fn connect(addr: &str) -> NetHandle {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<ClientMessage>();
    let (evt_tx, evt_rx) = crossbeam_channel::unbounded::<NetEvent>();
    let alive = Arc::new(AtomicBool::new(true));
    let alive_thread = alive.clone();
    let url = format!("ws://{addr}/ws");

    std::thread::spawn(move || {
        let (mut ws, _) = match tungstenite::connect(&url) {
            Ok(ok) => ok,
            Err(e) => {
                let _ = evt_tx.send(NetEvent::Disconnected(e.to_string()));
                return;
            }
        };
        set_nonblocking(&mut ws);
        let _ = evt_tx.send(NetEvent::Connected);
        let mut seq: u64 = 0;

        loop {
            if !alive_thread.load(Ordering::Relaxed) {
                let _ = ws.close(None);
                return;
            }
            // Outbound commands.
            let mut sent = false;
            while let Ok(cmd) = cmd_rx.try_recv() {
                seq += 1;
                let frame = encode(&Envelope::new(seq, cmd));
                if ws.send(Message::Text(frame.into())).is_err() {
                    let _ = evt_tx.send(NetEvent::Disconnected("send failed".into()));
                    return;
                }
                sent = true;
            }
            // Inbound frames.
            match ws.read() {
                Ok(Message::Text(text)) => match decode::<ServerMessage>(&text) {
                    Ok(env) => {
                        let _ = evt_tx.send(NetEvent::Message(env.msg));
                    }
                    Err(e) => {
                        let _ = evt_tx.send(NetEvent::Disconnected(format!("protocol error: {e}")));
                        return;
                    }
                },
                Ok(Message::Close(_)) => {
                    let _ = evt_tx.send(NetEvent::Disconnected("closed by server".into()));
                    return;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    if !sent {
                        std::thread::sleep(std::time::Duration::from_millis(15));
                    }
                }
                Err(e) => {
                    let _ = evt_tx.send(NetEvent::Disconnected(e.to_string()));
                    return;
                }
            }
        }
    });

    NetHandle {
        tx: cmd_tx,
        rx: evt_rx,
        alive,
    }
}

fn set_nonblocking(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
    if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
        let _ = stream.set_nonblocking(true);
    }
}

/// Fetch a board definition over HTTP (plain std, no async runtime).
pub fn fetch_board(addr: &str, board_id: &str) -> Result<realmweave_core::BoardDefinition, String> {
    // Called from frame-adjacent contexts: bound every network wait so a
    // hung server can never freeze the client indefinitely.
    let timeout = std::time::Duration::from_secs(3);
    let sock_addr = addr
        .parse()
        .or_else(|_| {
            use std::net::ToSocketAddrs;
            addr.to_socket_addrs()
                .map_err(|e| e.to_string())?
                .next()
                .ok_or_else(|| format!("no address for {addr}"))
        })
        .map_err(|e: String| e)?;
    let stream = TcpStream::connect_timeout(&sock_addr, timeout).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let mut stream = stream;
    use std::io::{Read, Write};
    let request =
        format!("GET /api/boards/{board_id} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| e.to_string())?;
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .ok_or("malformed HTTP response")?;
    serde_json::from_str(body).map_err(|e| e.to_string())
}
