use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::protocol::IpcMessage;

#[cfg(windows)]
use interprocess::os::windows::named_pipe::{PipeListenerOptions, PipeMode, pipe_mode};

#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\meowcal-sub";

#[cfg(unix)]
const PIPE_NAME: &str = "@meowcal-sub";

pub type IpcMessageHandler = Arc<dyn Fn(IpcMessage) + Send + Sync>;

#[cfg(windows)]
type SendHalf = interprocess::os::windows::named_pipe::SendPipeStream<pipe_mode::Bytes>;

pub struct IpcServer {
    handler: IpcMessageHandler,
    #[cfg(windows)]
    client: Arc<Mutex<Option<SendHalf>>>,
    #[cfg(not(windows))]
    client: Arc<Mutex<Option<()>>>,
}

impl IpcServer {
    pub fn new(handler: IpcMessageHandler) -> Self {
        Self {
            handler,
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the IPC server
    #[cfg(windows)]
    pub async fn start(&self) {
        info!("🔌 Starting IPC server on pipe: {}", PIPE_NAME);

        let listener = match PipeListenerOptions::new()
            .path(PIPE_NAME)
            .mode(PipeMode::Bytes)
            .create() {
            Ok(l) => l,
            Err(e) => {
                error!("❌ Failed to create pipe server: {}", e);
                return;
            }
        };

        info!("✅ IPC server listening for connections");

        loop {
            match listener.accept() {
                Ok(stream) => {
                    info!("🔗 OverlayHost connected");

                    // Split the stream into recv and send halves
                    let (recv_half, send_half) = stream.split();

                    // Store send half for later use
                    {
                        let mut client = self.client.lock().await;
                        *client = Some(send_half);
                    }

                    // Handle this client in a blocking task
                    let handler = self.handler.clone();
                    let client_ref = self.client.clone();

                    tokio::task::spawn_blocking(move || {
                        Self::handle_client(recv_half, handler, client_ref);
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    #[cfg(not(windows))]
    pub async fn start(&self) {
        warn!("IPC server is only supported on Windows");
    }

    #[cfg(windows)]
    fn handle_client(
        recv_half: interprocess::os::windows::named_pipe::RecvPipeStream<pipe_mode::Bytes>,
        handler: IpcMessageHandler,
        client_ref: Arc<Mutex<Option<SendHalf>>>,
    ) {
        // Use BufReader for line-based reading
        let mut reader = BufReader::new(recv_half);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    info!("OverlayHost disconnected (EOF)");
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<IpcMessage>(trimmed) {
                        Ok(message) => {
                            debug!("← Received from OverlayHost: {}", message.message_type);
                            handler(message);
                        }
                        Err(e) => {
                            warn!("Failed to parse IPC message '{}': {}", trimmed, e);
                        }
                    }
                }
                Err(e) => {
                    info!("OverlayHost disconnected: {}", e);
                    break;
                }
            }
        }

        // Clear client connection on disconnect
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut client = client_ref.lock().await;
                *client = None;
            });
        });
    }

    /// Send message to OverlayHost
    #[cfg(windows)]
    pub async fn send(&self, message: IpcMessage) {
        let mut client = self.client.lock().await;

        if let Some(send_half) = client.as_mut() {
            let json = match serde_json::to_string(&message) {
                Ok(j) => j,
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    return;
                }
            };

            let data = format!("{}\n", json);

            if let Err(e) = send_half.write_all(data.as_bytes()) {
                error!("❌ Failed to send to OverlayHost: {}", e);
            } else if let Err(e) = send_half.flush() {
                error!("❌ Failed to flush to OverlayHost: {}", e);
            } else {
                debug!("→ Sent to OverlayHost: {}", message.message_type);
            }
        } else {
            warn!("⚠️ Cannot send message: OverlayHost not connected");
        }
    }

    #[cfg(not(windows))]
    pub async fn send(&self, _message: IpcMessage) {
        warn!("IPC send is only supported on Windows");
    }
}
