use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};

use super::protocol::IpcMessage;

const PIPE_NAME: &str = "@meowcal-sub"; // Unix socket name (cross-platform)

#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\meowcal-sub";

pub type IpcMessageHandler = Arc<dyn Fn(IpcMessage) + Send + Sync>;

pub struct IpcServer {
    handler: IpcMessageHandler,
    client: Arc<Mutex<Option<LocalSocketStream>>>,
}

impl IpcServer {
    pub fn new(handler: IpcMessageHandler) -> Self {
        Self {
            handler,
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the IPC server
    pub async fn start(&self) {
        info!("🔌 Starting IPC server on pipe: {}", PIPE_NAME);

        // Remove existing pipe if it exists (Unix only)
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(PIPE_NAME);
        }

        let listener = match LocalSocketListener::bind(PIPE_NAME) {
            Ok(l) => l,
            Err(e) => {
                error!("❌ Failed to bind pipe server: {}", e);
                return;
            }
        };

        info!("✅ IPC server listening for connections");

        loop {
            match listener.accept() {
                Ok(stream) => {
                    info!("🔗 OverlayHost connected");

                    // Store client connection
                    {
                        let mut client = self.client.lock().await;
                        *client = Some(stream.try_clone().expect("Failed to clone stream"));
                    }

                    // Handle this client in a blocking task
                    let handler = self.handler.clone();
                    let client_ref = self.client.clone();

                    tokio::task::spawn_blocking(move || {
                        Self::handle_client(stream, handler, client_ref);
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    fn handle_client(
        stream: LocalSocketStream,
        handler: IpcMessageHandler,
        client_ref: Arc<Mutex<Option<LocalSocketStream>>>,
    ) {
        let reader = BufReader::new(stream);

        for line in reader.lines() {
            match line {
                Ok(line) => {
                    match serde_json::from_str::<IpcMessage>(&line) {
                        Ok(message) => {
                            debug!("← Received from OverlayHost: {}", message.message_type);
                            handler(message);
                        }
                        Err(e) => {
                            warn!("Failed to parse IPC message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    info!("OverlayHost disconnected: {}", e);
                    // Clear client connection
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            let mut client = client_ref.lock().await;
                            *client = None;
                        });
                    });
                    break;
                }
            }
        }
    }

    /// Send message to OverlayHost
    pub async fn send(&self, message: IpcMessage) {
        let mut client = self.client.lock().await;

        if let Some(stream) = client.as_mut() {
            let json = match serde_json::to_string(&message) {
                Ok(j) => j,
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    return;
                }
            };

            let data = format!("{}\n", json);

            if let Err(e) = stream.write_all(data.as_bytes()) {
                error!("❌ Failed to send to OverlayHost: {}", e);
                // Clear failed connection
                *client = None;
            } else {
                debug!("→ Sent to OverlayHost: {}", message.message_type);
            }
        } else {
            warn!("⚠️ Cannot send message: OverlayHost not connected");
        }
    }
}
