use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const API_VERSION: u32 = 1;
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
pub const CAPABILITIES: &[&str] = &[
    "status",
    "install",
    "ready",
    "complete",
    "shutdown",
    "ocrLanguages",
    "ocrInitialize",
    "ocrRecognizeBgra",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: u64,
    pub api: u32,
    pub method: String,
    pub params: serde_json::Map<String, Value>,
    #[serde(default, rename = "payloadBytes")]
    pub payload_bytes: u64,
    #[serde(skip)]
    pub payload: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl Error {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        // Domain errors contain paths and infrastructure diagnostics, never request text.
        let prefix = message.split(':').next().unwrap_or_default();
        let code = if !prefix.is_empty()
            && prefix
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
        {
            prefix
        } else {
            "ENGINE_ERROR"
        };
        Self::new(code, &message)
    }
}

pub fn decode(bytes: &[u8]) -> Result<Request, Error> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::new(
            "FRAME_TOO_LARGE",
            "Request exceeds the frame limit",
        ));
    }
    let request: Request = serde_json::from_slice(bytes)
        .map_err(|_| Error::new("INVALID_REQUEST", "Expected a Core request object"))?;
    if request.api != API_VERSION {
        return Err(Error::new("API_MISMATCH", "Unsupported Core API version"));
    }
    Ok(request)
}

pub fn response(id: u64, result: Result<Value, Error>) -> Value {
    match result {
        Ok(result) => json!({"id": id, "result": result}),
        Err(error) => json!({"id": id, "error": error}),
    }
}
