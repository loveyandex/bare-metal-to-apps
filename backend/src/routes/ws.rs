use crate::worker::AppState;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
};

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.hub.subscribe();
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(ev) => {
                        let Ok(text) = serde_json::to_string(&ev) else { continue };
                        if socket.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => continue,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}
