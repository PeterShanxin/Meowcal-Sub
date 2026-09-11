use meowcal_core::protocol::{self, Error, MAX_FRAME_BYTES, MAX_OCR_FRAME_BYTES};
use meowcal_core::service::Service;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};

fn write_frame(output: &Mutex<std::io::Stdout>, value: &Value) -> Result<(), ()> {
    let bytes = serde_json::to_vec(value).map_err(|_| ())?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(());
    }
    let mut output = output.lock().map_err(|_| ())?;
    output.write_all(&bytes).map_err(|_| ())?;
    output.write_all(b"\n").map_err(|_| ())?;
    output.flush().map_err(|_| ())
}

fn read_frame(input: &mut impl BufRead) -> Result<Option<Vec<u8>>, ()> {
    let mut frame = Vec::new();
    loop {
        let available = input.fill_buf().map_err(|_| ())?;
        if available.is_empty() {
            return if frame.is_empty() { Ok(None) } else { Err(()) };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if frame.len() + count > MAX_OCR_FRAME_BYTES {
            return Err(());
        }
        frame.extend_from_slice(&available[..count]);
        input.consume(count);
        if newline.is_some() {
            return Ok(Some(frame));
        }
    }
}

#[tokio::main]
async fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments == ["--version-json"] {
        println!(
            "{}",
            json!({"version":protocol::CORE_VERSION,"api":protocol::API_VERSION,"capabilities":protocol::CAPABILITIES})
        );
        return;
    }
    if !arguments.is_empty() {
        eprintln!("Usage: meowcal-core [--version-json]");
        std::process::exit(2);
    }
    let (sender, mut requests) = mpsc::channel(1);
    let (closed, mut disconnect) = watch::channel(false);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        while let Ok(Some(frame)) = read_frame(&mut input) {
            // One pending frame is the maximum; flooding closes the session.
            if sender.try_send(frame).is_err() {
                break;
            }
        }
        let _ = closed.send(true);
        // A consumer can close stdin while leaving stdout unread. Its full pipe
        // must not keep Core (and its owned runtime) alive indefinitely.
        std::thread::sleep(std::time::Duration::from_secs(2));
        std::process::exit(0);
    });
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let mut service = Service::default();
    loop {
        let frame = tokio::select! {
            biased;
            _ = disconnect.changed() => break,
            frame = requests.recv() => match frame { Some(frame) => frame, None => break },
        };
        let request = match protocol::decode(&frame) {
            Ok(request) => request,
            Err(error) => {
                let id = serde_json::from_slice::<Value>(&frame)
                    .ok()
                    .and_then(|value| value.get("id").and_then(Value::as_u64));
                let value = match id {
                    Some(id) => protocol::response(id, Err(error)),
                    None => json!({"id":null,"error":error}),
                };
                if write_frame(&output, &value).is_err() {
                    break;
                }
                continue;
            }
        };
        let id = request.id;
        let shutdown = request.method == "shutdown";
        let progress_output = Arc::clone(&output);
        let progress = move |message: String| {
            let _ = write_frame(
                &progress_output,
                &json!({"id":id,"event":"progress","message":message}),
            );
        };
        let result = tokio::select! {
            biased;
            _ = disconnect.changed() => break,
            result = service.handle(request, &progress) => result,
        };
        let fatal = result.as_ref().is_err_and(|error| {
            matches!(
                error.code.as_str(),
                "INSTALL_TIMEOUT" | "READY_TIMEOUT" | "OCR_TIMEOUT"
            )
        });
        let shutdown = shutdown && result.is_ok();
        let mut response = protocol::response(id, result);
        if serde_json::to_vec(&response).map_or(true, |bytes| bytes.len() > MAX_FRAME_BYTES) {
            response = protocol::response(
                id,
                Err(Error::new(
                    "RESPONSE_TOO_LARGE",
                    "Core response exceeds frame limit",
                )),
            );
        }
        if write_frame(&output, &response).is_err() || shutdown || fatal {
            break;
        }
    }
    meowcal_core::hy_mt_runtime::shutdown_owned();
    // A cancelled blocking hash, ZIP extraction, or native OCR call must not
    // outlive its session and mutate assets after its lease has been released.
    std::process::exit(0);
}
