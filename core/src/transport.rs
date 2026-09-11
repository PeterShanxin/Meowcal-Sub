use meowcal_core::protocol::{self, Request, MAX_FRAME_BYTES};
use serde_json::{json, Value};
use std::io::BufRead;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

const BODY_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Incoming {
    pub request: Result<Request, Value>,
    pub body_permit: Option<OwnedSemaphorePermit>,
}

fn read_header(input: &mut impl BufRead) -> Result<Option<Vec<u8>>, ()> {
    let mut frame = Vec::new();
    loop {
        let available = input.fill_buf().map_err(|_| ())?;
        if available.is_empty() {
            return if frame.is_empty() { Ok(None) } else { Err(()) };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if frame.len() + count > MAX_FRAME_BYTES {
            return Err(());
        }
        frame.extend_from_slice(&available[..count]);
        input.consume(count);
        if newline.is_some() {
            return Ok(Some(frame));
        }
    }
}

fn read_request(
    input: &mut impl BufRead,
    budget: &Arc<Semaphore>,
    deadline: &watch::Sender<Option<Instant>>,
) -> Result<Option<Incoming>, ()> {
    let Some(header) = read_header(input)? else {
        return Ok(None);
    };
    let mut request = match protocol::decode(&header) {
        Ok(request) => request,
        Err(error) => {
            // Malformed JSON or a malformed binary envelope cannot establish
            // where the next request starts. Never interpret its body as JSON.
            let value: Value = serde_json::from_slice(&header).map_err(|_| ())?;
            let control_header = serde_json::from_slice::<Request>(&header).is_ok_and(|request| {
                request.payload_bytes == 0 && request.method != "ocrRecognizeBgra"
            });
            if !value.is_object()
                || (value.get("payloadBytes").is_some() && !control_header)
                || value.get("method").and_then(Value::as_str) == Some("ocrRecognizeBgra")
            {
                return Err(());
            }
            let id = value.get("id").and_then(Value::as_u64);
            return Ok(Some(Incoming {
                request: Err(json!({"id":id,"error":error})),
                body_permit: None,
            }));
        }
    };
    let mut body_permit = None;
    if request.method == "ocrRecognizeBgra" {
        let length =
            meowcal_core::ocr_service::validate_frame(&request.params, request.payload_bytes)
                .map_err(|_| ())?;
        // This permit spans reading, queuing and execution, so a second large
        // request cannot allocate another body behind the active native call.
        body_permit = Some(Arc::clone(budget).try_acquire_owned().map_err(|_| ())?);
        let end = Instant::now() + BODY_TIMEOUT;
        deadline.send(Some(end)).map_err(|_| ())?;
        request.payload = vec![0; length];
        input.read_exact(&mut request.payload).map_err(|_| ())?;
        if Instant::now() >= end {
            return Err(());
        }
        deadline.send(None).map_err(|_| ())?;
    } else if request.payload_bytes != 0 {
        return Err(());
    }
    Ok(Some(Incoming {
        request: Ok(request),
        body_permit,
    }))
}

pub fn start() -> (mpsc::Receiver<Incoming>, watch::Receiver<bool>) {
    let (sender, requests) = mpsc::channel(1);
    let (closed, disconnect) = watch::channel(false);
    let (deadline_sender, mut deadline) = watch::channel(None);
    let timeout_closed = closed.clone();
    tokio::spawn(async move {
        loop {
            let current = *deadline.borrow_and_update();
            if let Some(end) = current {
                tokio::select! {
                    biased;
                    changed = deadline.changed() => if changed.is_err() { break; },
                    _ = tokio::time::sleep_until(end) => {
                        close_session(&timeout_closed);
                        break;
                    }
                }
            } else if deadline.changed().await.is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let budget = Arc::new(Semaphore::new(1));
        while let Ok(Some(request)) = read_request(&mut input, &budget, &deadline_sender) {
            if sender.try_send(request).is_err() {
                break;
            }
        }
        close_session(&closed);
    });
    (requests, disconnect)
}

fn close_session(closed: &watch::Sender<bool>) {
    let _ = closed.send(true);
    // A full stdout pipe or uncooperative native call cannot retain the owned
    // runtime after EOF, invalid framing, or an incomplete-body deadline.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(2));
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn body_is_exact_and_budget_covers_pending_request() {
        let pixels = b"\n\0{}\r\n\xff\0";
        let header = json!({"id":1,"api":1,"method":"ocrRecognizeBgra", "payloadBytes":8,
            "params":{"width":2,"height":1,"stride":8,"timeoutMs":1000}});
        let mut bytes = format!("{header}\n").into_bytes();
        bytes.extend_from_slice(pixels);
        let second_start = bytes.len();
        bytes.extend_from_slice(format!("{header}\n").as_bytes());
        bytes.extend_from_slice(pixels);
        let mut input = Cursor::new(bytes);
        let budget = Arc::new(Semaphore::new(1));
        let (deadline, _receiver) = watch::channel(None);
        let first = read_request(&mut input, &budget, &deadline)
            .unwrap()
            .unwrap();
        assert_eq!(first.request.as_ref().unwrap().payload, pixels);
        assert_eq!(input.position(), second_start as u64);
        assert!(read_request(&mut input, &budget, &deadline).is_err());
        assert_eq!(input.get_ref().len() as u64 - input.position(), 8);
        drop(first);
        assert_eq!(budget.available_permits(), 1);
    }
}
