use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use tokio::sync::broadcast;
use tracing::warn;

use crate::api::{AppState, FlowEvent, StepCommand};

pub(crate) async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    let rx = state.tx.subscribe();
    let step_tx = state.step_tx.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, rx, step_tx))
}

async fn handle_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<FlowEvent>,
    step_tx: broadcast::Sender<StepCommand>,
) {
    let mut closed = false;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        if let Ok(text) = serde_json::to_string(&event) {
                            if socket.send(Message::Text(text.into())).await.is_err() {
                                closed = true;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(lagged = n, "websocket client lagged behind");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        closed = true;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<StepCommand>(&text) {
                            let _ = step_tx.send(cmd);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        closed = true;
                    }
                    _ => {
                        closed = true;
                    }
                }
            }
        }

        if closed {
            break;
        }
    }
}
