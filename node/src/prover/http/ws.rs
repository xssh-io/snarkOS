// use crate::model::{StrategyData, StrategyDataLocal};
// use crate::monitor::server::model::MonitorConfig;
// use aide::axum::IntoApiResponse;
// use aide::transform::TransformOperation;
// use aide::NoApi;
// use axum::extract::ws::{Message, WebSocket};
// use axum::extract::State;
// use eyre::Result;
// use snarkos_node_router::Router;
// use snarkos_node_router_core::extractor::axum_client_ip::SecureClientIp;
// use snarkos_node_router_core::ws::WebSocketUpgrade;
// use std::net::IpAddr;
// use std::sync::Arc;
// use std::time::Duration;
// use tokio::sync::Mutex;
// use tokio::task::JoinHandle;
// use tracing::debug;
//
// pub async fn ws_handler(
//     ws: WebSocketUpgrade,
//     NoApi(SecureClientIp(addr)): NoApi<SecureClientIp>,
// ) -> impl IntoApiResponse {
//     ws.into_inner().on_upgrade(move |socket| {
//         WebsocketHandler::new(socket, addr, config.data.clone(), config.task.clone(), config.clients.clone()).run()
//     })
// }
// pub fn monitor_ws_handler_docs(op: TransformOperation) -> TransformOperation {
//     op.description("Monitor websocket handler")
// }
// #[derive(Clone)]
// pub struct WebsocketHandle {
//     addr: IpAddr,
//     tx: tokio::sync::mpsc::Sender<Message>,
//     data: Arc<StrategyData>,
// }
// impl WebsocketHandle {
//     pub async fn send_json<T: serde::Serialize>(&self, msg: T) -> Result<()> {
//         let addr = self.addr;
//         let text = serde_json::to_string(&msg)?;
//         debug!(?addr, "Sending: {}", text);
//         let msg = Message::Text(text);
//         self.tx.send(msg).await?;
//         Ok(())
//     }
//     pub async fn send_text(&self, text: String) -> Result<()> {
//         let addr = self.addr;
//         debug!(?addr, "Sending: {}", text);
//         let msg = Message::Text(text);
//         self.tx.send(msg).await?;
//         Ok(())
//     }
//     pub async fn send_raw(&self, data: MarketEventRaw) -> Result<()> {
//         let addr = self.addr;
//         if data.binary {
//             debug!(?addr, "Sending: {} bytes", data.data.len());
//             self.tx.send(Message::Binary(data.data)).await?;
//         } else {
//             debug!(?addr, "Sending: {}", unsafe { data.as_str() });
//             unsafe {
//                 self.tx.send(Message::Text(data.into_string())).await?;
//             }
//         }
//
//         Ok(())
//     }
//     pub async fn close(&self, frame: Option<axum::extract::ws::CloseFrame<'static>>) -> Result<()> {
//         self.tx.send(Message::Close(frame)).await?;
//         Ok(())
//     }
// }
// #[allow(dead_code)]
// pub struct WebsocketHandler {
//     socket: WebSocket,
//     addr: IpAddr,
//     rx: tokio::sync::mpsc::Receiver<Message>,
//     handle: WebsocketHandle,
//     task: Arc<Mutex<Option<JoinHandle<()>>>>,
//     clients: Arc<Mutex<Vec<WebsocketHandle>>>,
// }
// impl WebsocketHandler {
//     pub fn new(
//         socket: WebSocket,
//         addr: IpAddr,
//         data: Arc<StrategyData>,
//         task: Arc<Mutex<Option<JoinHandle<()>>>>,
//         clients: Arc<Mutex<Vec<WebsocketHandle>>>,
//     ) -> Self {
//         let (tx, rx) = tokio::sync::mpsc::channel(16);
//         Self { socket, addr, rx, handle: WebsocketHandle { addr, tx, data }, task, clients }
//     }
//
//     pub async fn run(self) {
//         self.clients.lock().await.push(self.handle.clone());
//         let lock = self.task.lock().await;
//         if lock.is_none() {
//             let clients = self.clients.clone();
//             let data = self.handle.data.clone();
//             let mut local_data = StrategyDataLocal::new();
//             tokio::spawn(async move {
//                 loop {
//                     local_data.extend_from_sync(&data);
//
//                     let msg = serde_json::to_string(&local_data.to_update()).unwrap();
//                     debug!("Sending message: {}", msg);
//
//                     let mut clients = clients.lock().await;
//                     let mut error_clients = vec![];
//                     for (i, client) in clients.iter().enumerate() {
//                         if client.send_text(msg.clone()).await.is_err() {
//                             error_clients.push(i);
//                         }
//                     }
//                     for i in error_clients.iter().rev() {
//                         clients.remove(*i);
//                     }
//
//                     tokio::time::sleep(Duration::from_millis(300)).await
//                 }
//             });
//         }
//     }
// }
