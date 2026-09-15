use super::*;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;

// Real HTTP responses keep process liveness green while inference degrades.
struct Engine {
    gpu: bool,
    endpoint: String,
    cpu_endpoint: String,
    starts: usize,
    stops: usize,
    samples: usize,
}

impl RecoveryEngine for Engine {
    fn lock_cpu(&mut self, progress: &(dyn Fn(String) + Send + Sync)) {
        progress(CPU_LOCK_EVENT.into());
    }
    async fn sample(&mut self) -> bool {
        self.samples += 1;
        let Ok(health) = reqwest::get(format!("{}/health", self.endpoint)).await else {
            return false;
        };
        assert!(health.status().is_success());
        completion::sample(&self.endpoint, "owned").await.is_ok()
    }
    async fn start_verified_cpu(
        &mut self,
        _progress: &(dyn Fn(String) + Send + Sync),
    ) -> Result<(), Error> {
        self.stop();
        self.gpu = false;
        self.starts += 1;
        self.endpoint = self.cpu_endpoint.clone();
        if self.sample().await {
            Ok(())
        } else {
            Err(Error::new("INFERENCE_FAILED", "CPU sample failed"))
        }
    }
    fn stop(&mut self) {
        self.stops += 1;
    }
}

#[tokio::test]
async fn vanished_gpu_still_gets_a_verified_cpu_replacement() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let vanished = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let (cpu, worker) = server("Leave the clock tower aside for now.", "stop");
    let mut engine = Engine {
        gpu: false,
        endpoint: vanished,
        cpu_endpoint: cpu,
        starts: 0,
        stops: 0,
        samples: 0,
    };
    let result = recover(&mut engine, true, false, &|_| {}).await;
    assert!(
        result.is_ok(),
        "GPU exit must not be classified as CPU failure"
    );
    assert_eq!((engine.starts, engine.samples), (1, 2));
    worker.join().unwrap();
}

fn server(content: &str, finish: &str) -> (String, std::thread::JoinHandle<()>) {
    server_sequence(&[(content, finish)])
}

#[tokio::test]
async fn failed_cpu_probe_is_terminal_without_another_start() {
    let (endpoint, worker) = server("Another clock tower.", "stop");
    let mut engine = Engine {
        gpu: false,
        endpoint,
        cpu_endpoint: String::new(),
        starts: 0,
        stops: 0,
        samples: 0,
    };
    let result = recover(&mut engine, false, false, &|_| {}).await;
    assert_eq!(result.unwrap_err().code, "INFERENCE_FAILED");
    assert_eq!((engine.starts, engine.stops, engine.samples), (0, 1, 1));
    worker.join().unwrap();
}

fn server_sequence(replies: &[(&str, &str)]) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let bodies: Vec<_> = replies.iter().map(|(content, finish)| json!({"choices":[{"message":{"content":content},"finish_reason":finish}],"usage":{"completion_tokens":12}}).to_string()).collect();
    let worker = std::thread::spawn(move || {
        for (index, stream) in listener.incoming().take(bodies.len() * 2).enumerate() {
            let mut stream = stream.unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let header = String::from_utf8(request).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            stream.read_exact(&mut vec![0; length]).unwrap();
            let response = if header.starts_with("GET /health") {
                "{}"
            } else {
                &bodies[index / 2]
            };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
        }
    });
    (endpoint, worker)
}

#[tokio::test]
async fn healthy_but_corrupt_gpu_switches_once_and_validates_cpu() {
    let (gpu, gpu_worker) = server_sequence(&[
        ("Let's not mention the clock tower yet.", "stop"),
        ("最后一班渡轮会在中午之前离开。---+", "length"),
    ]);
    let (cpu, cpu_worker) = server("Let's not mention the clock tower yet.", "stop");
    let mut engine = Engine {
        gpu: true,
        endpoint: gpu,
        cpu_endpoint: cpu,
        starts: 0,
        stops: 0,
        samples: 0,
    };
    assert!(
        engine.sample().await,
        "the same GPU initially passes its sample"
    );
    let events = std::sync::Mutex::new(Vec::new());
    recover(&mut engine, true, false, &|event| {
        events.lock().unwrap().push(event)
    })
    .await
    .unwrap();
    assert_eq!(
        (engine.starts, engine.stops, engine.samples, engine.gpu),
        (1, 1, 3, false)
    );
    assert_eq!(*events.lock().unwrap(), [CPU_LOCK_EVENT]);
    gpu_worker.join().unwrap();
    cpu_worker.join().unwrap();
}

#[tokio::test]
async fn sane_sample_does_not_restart_gpu_for_one_rejected_translation() {
    let (endpoint, worker) = server("Leave the clock tower aside for now.", "stop");
    let mut engine = Engine {
        gpu: true,
        endpoint,
        cpu_endpoint: String::new(),
        starts: 0,
        stops: 0,
        samples: 0,
    };
    recover(&mut engine, true, false, &|_| panic!("no CPU lock needed"))
        .await
        .unwrap();
    assert_eq!((engine.starts, engine.stops, engine.samples), (0, 0, 1));
    worker.join().unwrap();
}

#[tokio::test]
async fn repeated_strong_failures_skip_gpu_probe_and_cpu_failure_terminates() {
    let (cpu, worker) = server("tokens scores tokens scores", "length");
    let mut engine = Engine {
        gpu: true,
        endpoint: "http://127.0.0.1:1".into(),
        cpu_endpoint: cpu,
        starts: 0,
        stops: 0,
        samples: 0,
    };
    assert_eq!(
        recover(&mut engine, true, true, &|_| {})
            .await
            .unwrap_err()
            .code,
        "INFERENCE_FAILED"
    );
    assert_eq!((engine.starts, engine.stops, engine.samples), (1, 2, 1));
    worker.join().unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_stalled_cpu_load_ends_core_instead_of_leaving_install_work_alive() {
    struct StalledEngine {
        stopped: bool,
        locked: bool,
    }
    impl RecoveryEngine for StalledEngine {
        fn lock_cpu(&mut self, _: &(dyn Fn(String) + Send + Sync)) {
            self.locked = true;
        }
        async fn sample(&mut self) -> bool {
            false
        }
        async fn start_verified_cpu(
            &mut self,
            _: &(dyn Fn(String) + Send + Sync),
        ) -> Result<(), Error> {
            std::future::pending().await
        }
        fn stop(&mut self) {
            self.stopped = true;
        }
    }
    let mut engine = StalledEngine {
        stopped: false,
        locked: false,
    };
    let result = recover(&mut engine, true, false, &|_| {}).await;
    assert_eq!(result.unwrap_err().code, "READY_TIMEOUT");
    assert!(
        engine.locked && !engine.stopped,
        "the asset lease stays owned until Core exits"
    );
}
