use crate::communication::{protocol::*, types::*};
use crate::config::ini_parse;
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Instant};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, mpsc},
};

impl ServerState {
    fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }
    fn add(&mut self, addr: SocketAddr, tx: mpsc::Sender<Vec<u8>>) {
        self.clients.insert(
            addr,
            Client {
                addr,
                tx,
                connected_at: Instant::now(),
            },
        );
    }
    fn remove(&mut self, addr: &SocketAddr) {
        self.clients.remove(addr);
    }
    fn online_count(&self) -> usize {
        self.clients.len()
    }
    fn get_tx(&self, addr: &SocketAddr) -> Option<&mpsc::Sender<Vec<u8>>> {
        self.clients.get(addr).map(|c| &c.tx)
    }
}

type SharedState = Arc<Mutex<ServerState>>;
struct Client {
    addr: SocketAddr,
    tx: mpsc::Sender<Vec<u8>>, //状态收集，广播所有用户
    connected_at: Instant,
}
struct ServerState {
    clients: HashMap<SocketAddr, Client>,
}
pub async fn tcp_server_start() -> Option<i32> {
    let mut ip_port = ini_parse::ini_get_ini_config("network", "local_ip")?;
    ip_port.push_str(":9999");

    let shared_state = SharedState::new(Mutex::new(ServerState::new()));

    let listener = TcpListener::bind(ip_port).await.unwrap();
    loop {
        let (tcp_stream, socket_addr) = listener.accept().await.unwrap();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(32);

        let (recv, send) = tcp_stream.into_split();

        let tmp_shared_state = shared_state.clone();

        let mut serverstate = shared_state.lock().await;
        serverstate.add(socket_addr, tx.clone());
        println!(
            "New connection: {}, connecting num:{}",
            serverstate.clients.len(),
            serverstate.online_count()
        );
        tokio::spawn(async move {
            process(recv, tx, tmp_shared_state, &socket_addr).await;
        });
        tokio::spawn(async move {
            server_send(send, rx).await;
        });
    }
}

async fn process(
    recv: tokio::net::tcp::OwnedReadHalf,
    tx: mpsc::Sender<Vec<u8>>,
    shared_state: SharedState,
    addr : &SocketAddr
) {
    let mut recv = recv;
    let mut tx = tx;
    loop {
        let mut buf = [0; 1024];
        match recv.read(&mut buf).await {
            Ok(0) | Err(_) => {shared_state.lock().await.remove(addr); break},
            Ok(n) => {
                protocol_parse(&buf, &tx).await;
            }
        }
        // let data = &rx.recv().await;
    }
}
async fn server_send(send: tokio::net::tcp::OwnedWriteHalf, rx: mpsc::Receiver<Vec<u8>>) {
    let mut rx = rx;
    let mut send = send;
    while let Some(data) = rx.recv().await {
        if send.write_all(&data).await.is_err() {
            break;
        };
    }
}
