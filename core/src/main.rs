mod transport;

use meowcal_core::protocol::{self, Error, MAX_FRAME_BYTES};
use meowcal_core::service::Service;
use serde_json::{json, Value};
use std::io::Write;
use std::sync::{Arc, Mutex};

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
    let (mut requests, mut disconnect) = transport::start();
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let mut service = Service::default();
    loop {
        let incoming = tokio::select! {
            biased;
            _ = disconnect.changed() => break,
            frame = requests.recv() => match frame { Some(frame) => frame, None => break },
        };
        let transport::Incoming {
            request,
            body_permit,
        } = incoming;
        let request = match request {
            Ok(request) => request,
            Err(value) => {
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
                "INSTALL_TIMEOUT" | "READY_TIMEOUT" | "OCR_TIMEOUT" | "OCR_PROCESS_UNUSABLE"
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
        drop(body_permit);
        if write_frame(&output, &response).is_err() || shutdown || fatal {
            break;
        }
    }
    meowcal_core::hy_mt_runtime::shutdown_owned();
    // A cancelled blocking hash, ZIP extraction, or native OCR call must not
    // outlive its session and mutate assets after its lease has been released.
    std::process::exit(0);
}
