use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tracing::{debug, error, info, warn};

use super::protocol::IpcMessage;

#[cfg(windows)]
use interprocess::os::windows::named_pipe::{pipe_mode, PipeListenerOptions, PipeMode};
#[cfg(windows)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(windows)]
use tokio::sync::mpsc;

#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\meowcal-sub";

#[cfg(unix)]
const PIPE_NAME: &str = "@meowcal-sub";

pub type IpcMessageHandler = Arc<dyn Fn(IpcMessage) + Send + Sync>;

#[cfg(windows)]
#[derive(Clone)]
struct ConnectedClient {
    id: u64,
    tx: mpsc::UnboundedSender<IpcMessage>,
}

pub struct IpcServer {
    handler: IpcMessageHandler,

    // We only ever have one OverlayHost, but we still guard against overlapping reconnects.
    #[cfg(windows)]
    client: Arc<Mutex<Option<ConnectedClient>>>,
    #[cfg(windows)]
    next_client_id: AtomicU64,

    #[cfg(not(windows))]
    client: Arc<Mutex<Option<()>>>,
}

impl IpcServer {
    pub fn new(handler: IpcMessageHandler) -> Self {
        Self {
            handler,
            #[cfg(windows)]
            client: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            next_client_id: AtomicU64::new(1),
            #[cfg(not(windows))]
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// Returns true if OverlayHost is currently connected.
    pub fn is_connected(&self) -> bool {
        let client = match self.client.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("⚠️ IPC client mutex poisoned; recovering");
                poisoned.into_inner()
            }
        };
        client.is_some()
    }

    /// Start the IPC server
    #[cfg(windows)]
    pub async fn start(&self) {
        info!("🔌 Starting IPC server on pipe: {}", PIPE_NAME);

        let listener = match PipeListenerOptions::new()
            .path(PIPE_NAME)
            .mode(PipeMode::Bytes)
            .create_tokio_duplex::<pipe_mode::Bytes>()
        {
            Ok(l) => l,
            Err(e) => {
                error!("❌ Failed to create pipe server: {}", e);
                return;
            }
        };

        info!("✅ IPC server listening for connections");

        loop {
            match listener.accept().await {
                Ok(stream) => {
                    let client_id = self.next_client_id.fetch_add(1, Ordering::SeqCst);
                    info!("🔗 OverlayHost connected (client #{})", client_id);

                    let (tx, rx) = mpsc::unbounded_channel::<IpcMessage>();
                    {
                        let mut client = match self.client.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                warn!("⚠️ IPC client mutex poisoned; recovering");
                                poisoned.into_inner()
                            }
                        };
                        *client = Some(ConnectedClient {
                            id: client_id,
                            tx: tx.clone(),
                        });
                    }

                    let handler = Arc::clone(&self.handler);
                    let client_ref = Arc::clone(&self.client);

                    tokio::spawn(async move {
                        Self::handle_client(client_id, stream, handler, client_ref, rx).await;
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        }
    }

    #[cfg(not(windows))]
    pub async fn start(&self) {
        warn!("IPC server is only supported on Windows");
    }

    #[cfg(windows)]
    async fn handle_client(
        client_id: u64,
        stream: interprocess::os::windows::named_pipe::tokio::DuplexPipeStream<pipe_mode::Bytes>,
        handler: IpcMessageHandler,
        client_ref: Arc<Mutex<Option<ConnectedClient>>>,
        mut rx: mpsc::UnboundedReceiver<IpcMessage>,
    ) {
        let (recv_half, mut send_half) = stream.split();
        let mut reader = BufReader::new(recv_half);
        let mut line = String::new();

        loop {
            tokio::select! {
                read_res = reader.read_line(&mut line) => {
                    match read_res {
                        Ok(0) => {
                            info!("OverlayHost disconnected (EOF) (client #{})", client_id);
                            break;
                        }
                        Ok(_) => {
                            let trimmed = line.trim().to_string();
                            line.clear();
                            if trimmed.is_empty() {
                                continue;
                            }

                            match serde_json::from_str::<IpcMessage>(&trimmed) {
                                Ok(message) => {
                                    debug!(
                                        "← Received from OverlayHost: {} (client #{})",
                                        message.message_type,
                                        client_id
                                    );
                                    handler(message);
                                }
                                Err(e) => {
                                    warn!("Failed to parse IPC message '{}': {}", trimmed, e);
                                }
                            }
                        }
                        Err(e) => {
                            info!("OverlayHost disconnected: {} (client #{})", e, client_id);
                            break;
                        }
                    }
                }

                maybe_msg = rx.recv() => {
                    let Some(message) = maybe_msg else {
                        // Sender dropped (probably replaced by a newer connection).
                        break;
                    };

                    let message_type = message.message_type.clone();
                    match serde_json::to_string(&message) {
                        Ok(json) => {
                            let data = format!("{}\n", json);
                            if let Err(e) = send_half.write_all(data.as_bytes()).await {
                                error!(
                                    "❌ Failed to send to OverlayHost: {} (client #{})",
                                    e, client_id
                                );
                                break;
                            }
                            if let Err(e) = send_half.flush().await {
                                error!(
                                    "❌ Failed to flush to OverlayHost: {} (client #{})",
                                    e, client_id
                                );
                                break;
                            }
                            debug!("→ Sent to OverlayHost: {} (client #{})", message_type, client_id);
                        }
                        Err(e) => {
                            error!("Failed to serialize message '{}': {}", message_type, e);
                        }
                    }
                }
            }
        }

        // Clear client connection on disconnect, but only if we're still the active client.
        let mut client = match client_ref.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("⚠️ IPC client mutex poisoned; recovering");
                poisoned.into_inner()
            }
        };

        if client.as_ref().is_some_and(|c| c.id == client_id) {
            *client = None;
        }
    }

    /// Send message to OverlayHost
    #[cfg(windows)]
    pub async fn send(&self, message: IpcMessage) -> bool {
        let (client_id, sender) = {
            let client = match self.client.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("⚠️ IPC client mutex poisoned; recovering");
                    poisoned.into_inner()
                }
            };
            match client.as_ref() {
                Some(c) => (Some(c.id), Some(c.tx.clone())),
                None => (None, None),
            }
        };

        match (client_id, sender) {
            (Some(client_id), Some(sender)) => {
                if sender.send(message).is_err() {
                    warn!("⚠️ Failed to queue IPC message (client #{})", client_id);
                    let mut client = match self.client.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!("⚠️ IPC client mutex poisoned; recovering");
                            poisoned.into_inner()
                        }
                    };
                    if client.as_ref().is_some_and(|c| c.id == client_id) {
                        *client = None;
                    }
                    return false;
                }
                true
            }
            _ => {
                warn!("⚠️ Cannot send message: OverlayHost not connected");
                false
            }
        }
    }

    #[cfg(not(windows))]
    pub async fn send(&self, _message: IpcMessage) -> bool {
        warn!("IPC send is only supported on Windows");
        false
    }
}
