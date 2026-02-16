//! WebSocket types re-exported from [`tokio_tungstenite`].
//!
//! Register a WebSocket route via [`Router::ws`](crate::Router::ws) and
//! receive a [`WebSocketStream`] in the handler:
//!
//! ```
//! # use webserver::Router;
//! let mut router = Router::new();
//! router.ws("/ws", |_req, ws| async move {
//!     use futures_util::{SinkExt, StreamExt};
//!     use webserver::websocket::WsMessage;
//!
//!     let (mut tx, mut rx) = futures_util::StreamExt::split(ws);
//!     while let Some(Ok(msg)) = rx.next().await {
//!         if msg.is_text() || msg.is_binary() {
//!             let _ = tx.send(msg).await;
//!         }
//!     }
//! });
//! ```

/// A WebSocket message (text, binary, ping, pong, or close).
pub use tokio_tungstenite::tungstenite::Message as WsMessage;

/// A server-side WebSocket stream over a TCP connection.
pub type WebSocketStream = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

/// Compute the `Sec-WebSocket-Accept` value from a client key.
pub fn derive_accept_key(key: &str) -> String {
    tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes())
}
