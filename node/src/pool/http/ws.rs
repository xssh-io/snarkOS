use aide::axum::IntoApiResponse;
use aide::transform::TransformOperation;
use aide::NoApi;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::State;
use eyre::Result;
use snarkos_node_router_core::extractor::ip::SecureClientIp;
use snarkos_node_router_core::ws::WebSocketUpgrade;
use std::fmt::Debug;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};
type WsMessage = ();
#[derive(Clone)]
pub struct WsConfig {
    pub clients: Arc<Mutex<Vec<WebsocketHandle>>>,
}
impl WsConfig {
    pub fn new() -> Self {
        Self { clients: Arc::new(Mutex::new(Vec::new())) }
    }
    pub fn broadcast(&self, msg: WsMessage) -> JoinHandle<()> {
        let clients = self.clients.clone();
        tokio::spawn(async move {
            let text = serde_json::to_string(&msg).unwrap();
            let mut clients = clients.lock().await;
            let mut i = 0;
            while i < clients.len() {
                if clients[i].send_text(text.clone()).await.is_err() {
                    clients.remove(i);
                } else {
                    i += 1;
                }
            }
        })
    }
}
impl Debug for WsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonitorConfig").finish_non_exhaustive()
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    NoApi(SecureClientIp(addr)): NoApi<SecureClientIp>,
    config: State<WsConfig>,
) -> impl IntoApiResponse {
    ws.into_inner().on_upgrade(move |socket| WebsocketHandler::new(socket, addr, config.clients.clone()).run())
}
pub fn ws_handler_docs(op: TransformOperation) -> TransformOperation {
    op.description("Monitor websocket handler")
}

static CONNECTION_ID: AtomicU64 = AtomicU64::new(0);
#[derive(Clone)]
pub struct WebsocketHandle {
    id: u64,
    addr: IpAddr,
    tx: tokio::sync::mpsc::Sender<Message>,
}
impl WebsocketHandle {
    pub async fn send_json<T: serde::Serialize>(&self, msg: T) -> Result<()> {
        let addr = self.addr;
        let text = serde_json::to_string(&msg)?;
        debug!(?addr, "Sending: {}", text);
        let msg = Message::Text(text);
        self.tx.send(msg).await?;
        Ok(())
    }
    pub async fn send_text(&self, text: String) -> Result<()> {
        let addr = self.addr;
        debug!(?addr, "Sending: {}", text);
        let msg = Message::Text(text);
        self.tx.send(msg).await?;
        Ok(())
    }

    pub async fn close(&self, frame: Option<axum::extract::ws::CloseFrame<'static>>) -> Result<()> {
        self.tx.send(Message::Close(frame)).await?;
        Ok(())
    }
}

pub struct WebsocketHandler {
    id: u64,
    socket: WebSocket,
    #[allow(dead_code)]
    addr: IpAddr,
    rx: tokio::sync::mpsc::Receiver<Message>,
    handle: WebsocketHandle,
    clients: Arc<Mutex<Vec<WebsocketHandle>>>,
}
impl WebsocketHandler {
    pub fn new(socket: WebSocket, addr: IpAddr, clients: Arc<Mutex<Vec<WebsocketHandle>>>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let id = CONNECTION_ID.fetch_add(1, Ordering::AcqRel);
        Self { id, socket, addr, rx, handle: WebsocketHandle { id, addr, tx }, clients }
    }

    pub async fn run(mut self) {
        self.clients.lock().await.push(self.handle.clone());

        let mut interval = interval(Duration::from_millis(300));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            select! {
                Some(msg) = self.rx.recv() => {
                    if self.socket.send(msg).await.is_err() {
                        break
                    }
                }
                else => {
                    break
                }
            }
        }
        self.clients.lock().await.retain(|x| x.id != self.id);
    }
}
