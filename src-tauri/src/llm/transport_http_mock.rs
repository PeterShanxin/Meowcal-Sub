// =============================================================================
// TRANSPORT_HTTP_MOCK.RS - In-process loopback server for transport tests
// =============================================================================
// The engine contract is exercised against a tiny TCP server on an ephemeral
// loopback port. No external network, no Foundry installation, no CLI spawn:
// every request is driven through `config.endpoint_url`, which is exactly the
// seam the real managed runtime uses.
//
// This file is test-only. It is compiled via `#[path]` from `transport_http.rs`
// under `#[cfg(test)]`, so it never ships in the binary.
// =============================================================================

use crate::config::FoundryLocalConfig;
use crate::llm::foundry_local::FoundryLocalBackend;
use crate::sync_utils::lock_or_recover;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug)]
pub(super) struct MockRequest {
    pub(super) path: String,
    pub(super) body: String,
}

#[derive(Clone)]
pub(super) struct MockResponse {
    status: u16,
    body: String,
    delay_ms: u64,
}

impl MockResponse {
    pub(super) fn ok(body: &str) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            delay_ms: 0,
        }
    }

    pub(super) fn status_only(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
            delay_ms: 0,
        }
    }

    pub(super) fn delayed(status: u16, body: &str, delay_ms: u64) -> Self {
        Self {
            status,
            body: body.to_string(),
            delay_ms,
        }
    }
}

pub(super) struct MockServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<MockRequest>>>,
}

impl MockServer {
    pub(super) async fn start(
        handler: impl Fn(&str) -> MockResponse + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(handler);

        let task_requests = Arc::clone(&requests);
        let task_handler = Arc::clone(&handler);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let requests = Arc::clone(&task_requests);
                let handler = Arc::clone(&task_handler);
                tokio::spawn(async move {
                    let Some(request) = read_request(&mut socket).await else {
                        return;
                    };
                    let path = request.path.clone();
                    lock_or_recover(&requests).push(request);
                    let response = handler(&path);
                    if response.delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(response.delay_ms))
                            .await;
                    }
                    let _ = socket
                        .write_all(format_response(&response).as_bytes())
                        .await;
                });
            }
        });

        Ok(Self { addr, requests })
    }

    /// Server that accepts a connection and immediately closes it.
    pub(super) async fn start_closing() -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let _ = socket.shutdown().await;
            }
        });
        Ok(Self { addr, requests })
    }

    pub(super) fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.addr.port())
    }

    pub(super) fn request_paths(&self) -> Vec<String> {
        lock_or_recover(&self.requests)
            .iter()
            .map(|request| request.path.clone())
            .collect()
    }

    pub(super) fn request_bodies(&self) -> Vec<String> {
        lock_or_recover(&self.requests)
            .iter()
            .map(|request| request.body.clone())
            .collect()
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<MockRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = socket.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buffer[..position]);
            let mut lines = head.split("\r\n");
            let request_line = lines.next()?;
            let path = request_line.split_whitespace().nth(1)?.to_string();
            let mut content_length = 0usize;
            for line in lines {
                let lower = line.to_ascii_lowercase();
                if let Some((_, value)) = lower.split_once("content-length:") {
                    content_length = value.trim().parse().ok()?;
                }
            }
            let body_start = position + 4;
            while buffer.len() < body_start + content_length {
                let n = socket.read(&mut chunk).await.ok()?;
                if n == 0 {
                    return None;
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
            let body = String::from_utf8_lossy(&buffer[body_start..body_start + content_length])
                .to_string();
            return Some(MockRequest { path, body });
        }
    }
}

fn format_response(response: &MockResponse) -> String {
    let reason = match response.status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    };
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        reason,
        response.body.len(),
        response.body
    )
}

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

pub(super) fn backend_for(url: &str, model: &str, timeout_ms: u32) -> FoundryLocalBackend {
    FoundryLocalBackend::new(FoundryLocalConfig {
        model: Some(model.to_string()),
        endpoint_url: Some(url.to_string()),
        timeout_ms,
        ..FoundryLocalConfig::default()
    })
}

/// The probe cache is a process-global static keyed by service URL + model.
/// Tests that drive `FoundryLocalBackend` serialize on this lock so one test's
/// probe recording cannot race another test's snapshot read.
static PROBE_CACHE_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn lock_probe_cache() -> std::sync::MutexGuard<'static, ()> {
    lock_or_recover(&PROBE_CACHE_LOCK)
}

pub(super) fn models_json(ids: &[&str]) -> String {
    let data: Vec<String> = ids
        .iter()
        .map(|id| format!(r#"{{"id":"{id}","object":"model"}}"#))
        .collect();
    format!(r#"{{"data":[{}]}}"#, data.join(","))
}

pub(super) fn chat_json(content: &str) -> String {
    format!(r#"{{"choices":[{{"message":{{"role":"assistant","content":"{content}"}}}}]}}"#)
}
