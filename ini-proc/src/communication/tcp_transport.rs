use crate::config::ini_parse; 
use std::net::SocketAddr;
use std::time::Instant;
use std::collections::HashMap;
use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::Mutex;

impl ServerState {
    fn new() -> Self {
        Self { clients: HashMap::new() }
    }
    fn add(&mut self, addr: SocketAddr, tx: mpsc::Sender<Vec<u8>>) {
        self.clients.insert(addr, Client { addr, tx, connected_at: Instant::now() });
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
    tx: mpsc::Sender<Vec<u8>>,
    connected_at: Instant,
}
struct ServerState {
    clients: HashMap<SocketAddr, Client>,
}
pub async fn tcp_server_start() -> Option<i32>{
    let mut ip_port = ini_parse::ini_get_ini_config("network", "local_ip")?;
    ip_port.push_str(":9999");

    let shared_state = SharedState::new(Mutex::new(ServerState::new()));

    let listener = TcpListener::bind(ip_port).await.unwrap();
    loop {
        let (tcp_stream, socket_addr) = listener.accept().await.unwrap();
        let (tx,rx) = mpsc::channel::<Vec<u8>>(32);
        tokio::spawn(async move {
            process(tcp_stream).await;
        });
    }
    Some(3)
}

async fn process(socket: TcpStream) {

}