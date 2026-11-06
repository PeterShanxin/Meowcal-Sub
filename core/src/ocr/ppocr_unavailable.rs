use super::{NativeResult, OcrError};

/// This Core was built without the `ppocr` feature; no instance can be created.
pub struct PpOcr {
    _private: (),
}

impl PpOcr {
    pub fn new() -> Result<Self, OcrError> {
        Err(OcrError::Init(
            "this Core was built without PP-OCRv6".into(),
        ))
    }

    pub fn recognize_bgra(
        &self,
        _bytes: &[u8],
        _width: u32,
        _height: u32,
    ) -> Result<NativeResult, OcrError> {
        Err(OcrError::Init(
            "this Core was built without PP-OCRv6".into(),
        ))
    }
}
