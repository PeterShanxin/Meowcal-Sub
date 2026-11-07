//! PP-OCRv6 small text detection and recognition on the CPU, through RTen.
//!
//! The models are RapidOCR's ONNX conversions of PaddleOCR's PP-OCRv6 small
//! detector and multilingual recognizer (Apache-2.0), described in
//! `core/models/ppocr/README.md`. They are compiled into the executable, so the
//! reviewed Core archive digest authenticates them like the rest of Core.
//!
//! Pre- and post-processing follow RapidOCR's defaults, the configuration
//! measured in `docs/plans/2026-11-05-ocr-stability-experiment.md`, except:
//! - there is no text-orientation classifier;
//! - wide strips are not padded with black rows before detection. On subtitle
//!   strips that padding doubled detection time and made the detector read the
//!   whole strip as one line of invented text;
//! - detection boxes are axis-aligned and sized differently, as `detect.rs`
//!   describes.

mod detect;
mod image;
mod recognize;

use super::{frame_bytes, LineBox, NativeResult, OcrError, PP_OCR_MAX_THREADS};
use detect::TextBox;
use image::Bgr;
use rten::{Model, ModelOptions, RunOptions, ThreadPool};
use rten_tensor::prelude::*;
use rten_tensor::Tensor;
use std::sync::Arc;

static DETECTOR: &[u8] = include_bytes!("../../../models/ppocr/PP-OCRv6_det_small.onnx");
static RECOGNIZER: &[u8] = include_bytes!("../../../models/ppocr/PP-OCRv6_rec_small.onnx");
static CHARSET: &str = include_str!("../../../models/ppocr/PP-OCRv6_rec_small_dict.txt");

/// Lines whose mean character probability is lower are discarded.
const MIN_LINE_SCORE: f32 = 0.5;
/// Images are resized into these side bounds before detection.
const MIN_SIDE: usize = 30;
const MAX_SIDE: usize = 2000;

pub struct PpOcr {
    detector: Model,
    recognizer: Model,
    charset: Vec<String>,
    threads: usize,
    options: RunOptions,
}

/// A third of the machine's cores, within `1..=PP_OCR_MAX_THREADS`.
fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |count| count.get() / 3)
        .clamp(1, PP_OCR_MAX_THREADS)
}

fn run_options(threads: usize) -> RunOptions {
    RunOptions::default().with_thread_pool(Some(Arc::new(ThreadPool::with_num_threads(threads))))
}

impl PpOcr {
    pub fn new() -> Result<Self, OcrError> {
        let load = |bytes: &'static [u8]| {
            ModelOptions::with_all_ops()
                .load_static_slice(bytes)
                .map_err(|error| OcrError::Init(error.to_string()))
        };
        let threads = default_threads();
        Ok(Self {
            detector: load(DETECTOR)?,
            recognizer: load(RECOGNIZER)?,
            // CTC class 0 is the blank; the space class follows the dictionary.
            charset: CHARSET.lines().chain([" "]).map(str::to_owned).collect(),
            threads,
            options: run_options(threads),
        })
    }

    /// Use at most `limit` threads, or the default share of the machine when
    /// `None`. The caller knows whether the translation engine needs the CPU;
    /// only the thread pool changes, so the models stay loaded.
    pub fn limit_threads(&mut self, limit: Option<usize>) {
        let threads = limit
            .unwrap_or_else(default_threads)
            .clamp(1, PP_OCR_MAX_THREADS);
        if threads != self.threads {
            self.threads = threads;
            self.options = run_options(threads);
        }
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn recognize_bgra(
        &self,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> Result<NativeResult, OcrError> {
        let expected = frame_bytes(width, height, width.saturating_mul(4))?;
        if bytes.len() != expected {
            return Err(OcrError::Frame(
                "byte length does not match dimensions".into(),
            ));
        }
        let original = Bgr::from_bgra(bytes, width as usize, height as usize);
        let bounded = within_bounds(&original);
        let found = self.detect(&bounded)?;
        let crops: Vec<Bgr> = found.iter().map(|text| crop(&bounded, text)).collect();
        let read = self.recognize(&crops)?;

        let scale_x = original.width as f32 / bounded.width as f32;
        let scale_y = original.height as f32 / bounded.height as f32;
        let mut lines = Vec::new();
        let mut boxes = Vec::new();
        for (text_box, (text, score)) in found.iter().zip(read) {
            if score < MIN_LINE_SCORE || text.trim().is_empty() {
                continue;
            }
            let y = |value: i32| (value as f32 * scale_y).min(original.height as f32);
            let x = |value: i32| (value as f32 * scale_x).min(original.width as f32);
            lines.push(text);
            boxes.push(LineBox {
                x: x(text_box.x0),
                y: y(text_box.y0),
                width: x(text_box.x1) - x(text_box.x0),
                height: y(text_box.y1) - y(text_box.y0),
            });
        }
        Ok(NativeResult {
            text: lines.join("\n"),
            lines,
            boxes,
            frame_width: width as f32,
        })
    }

    fn detect(&self, image: &Bgr) -> Result<Vec<TextBox>, OcrError> {
        let Some((width, height)) = detect::input_size(image.width, image.height) else {
            return Ok(Vec::new());
        };
        let mut data = vec![0.0; 3 * width * height];
        image
            .resize(width, height)
            .write_normalized(&mut data, width);
        let output = self.run(
            &self.detector,
            Tensor::from_data(&[1, 3, height, width], data),
        )?;
        let shape = output.shape().to_vec();
        if shape.len() != 4 || shape[0] != 1 || shape[1] != 1 {
            return Err(recognition_error(format!(
                "detector output shape {shape:?}"
            )));
        }
        let probability = output.to_vec();
        Ok(detect::boxes(
            &probability,
            shape[3],
            shape[2],
            image.width,
            image.height,
        ))
    }

    /// Read every crop, batched by similar width as the reference does so the
    /// zero padding each line sees matches it.
    fn recognize(&self, crops: &[Bgr]) -> Result<Vec<(String, f32)>, OcrError> {
        let mut order: Vec<usize> = (0..crops.len()).collect();
        order.sort_by(|&a, &b| {
            let ratio = |crop: &Bgr| crop.width as f64 / crop.height as f64;
            ratio(&crops[a]).total_cmp(&ratio(&crops[b]))
        });
        let mut read = vec![(String::new(), 0.0); crops.len()];
        for chunk in order.chunks(recognize::BATCH_SIZE) {
            let members: Vec<&Bgr> = chunk.iter().map(|&index| &crops[index]).collect();
            let batch = recognize::batch(&members);
            let input = Tensor::from_data(
                &[batch.count, 3, recognize::LINE_HEIGHT, batch.width],
                batch.data,
            );
            let output = self.run(&self.recognizer, input)?;
            let shape = output.shape().to_vec();
            if shape.len() != 3 || shape[0] != batch.count || shape[2] != self.charset.len() + 1 {
                return Err(recognition_error(format!(
                    "recognizer output shape {shape:?}"
                )));
            }
            let probabilities = output.to_vec();
            let row = shape[1] * shape[2];
            for (position, &index) in chunk.iter().enumerate() {
                read[index] = recognize::decode(
                    &probabilities[position * row..(position + 1) * row],
                    shape[2],
                    &self.charset,
                );
            }
        }
        Ok(read)
    }

    fn run(&self, model: &Model, input: Tensor<f32>) -> Result<Tensor<f32>, OcrError> {
        model
            .run_one(input.view().into(), Some(self.options.clone()))
            .map_err(|error| recognition_error(error.to_string()))?
            .into_tensor::<f32>()
            .ok_or_else(|| recognition_error("model output is not a float tensor".into()))
    }
}

fn recognition_error(message: String) -> OcrError {
    OcrError::Recognition(message)
}

/// Resize into `MIN_SIDE..=MAX_SIDE`, rounding both sides to multiples of 32
/// whenever a resize happens.
fn within_bounds(image: &Bgr) -> Bgr {
    let mut bounded = image.clone();
    let fit = |ratio: f64, current: &Bgr| {
        let side = |value: usize| detect::round_to_32((value as f64 * ratio) as usize).max(32);
        current.resize(side(current.width), side(current.height))
    };
    let longest = bounded.width.max(bounded.height);
    if longest > MAX_SIDE {
        bounded = fit(MAX_SIDE as f64 / longest as f64, &bounded);
    }
    let shortest = bounded.width.min(bounded.height);
    if shortest < MIN_SIDE {
        bounded = fit(MIN_SIDE as f64 / shortest as f64, &bounded);
    }
    bounded
}

/// The box's pixels, turned upright when it is much taller than wide.
fn crop(image: &Bgr, text: &TextBox) -> Bgr {
    let (x, y) = (text.x0 as usize, text.y0 as usize);
    let width = (text.x1 - text.x0) as usize;
    let height = (text.y1 - text.y0) as usize;
    let crop = image.crop(x, y, width, height);
    if height as f64 / width as f64 >= 1.5 {
        crop.rotate_counter_clockwise()
    } else {
        crop
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
