//! Text-line recognition input and CTC decoding.

use super::image::Bgr;

pub const LINE_HEIGHT: usize = 48;
/// Narrowest batch width, as a width-to-height ratio (320 / 48).
const MIN_RATIO: f64 = 320.0 / 48.0;
pub const BATCH_SIZE: usize = 6;

pub struct Batch {
    pub data: Vec<f32>,
    pub count: usize,
    pub width: usize,
}

/// Resize each crop to the line height and pad all of them to the widest,
/// as one `[count, 3, 48, width]` tensor.
pub fn batch(crops: &[&Bgr]) -> Batch {
    let ratio = |crop: &Bgr| crop.width as f64 / crop.height as f64;
    let widest = crops
        .iter()
        .map(|crop| ratio(crop))
        .fold(MIN_RATIO, f64::max);
    let width = (LINE_HEIGHT as f64 * widest) as usize;
    let item = 3 * LINE_HEIGHT * width;
    let mut data = vec![0.0; crops.len() * item];
    for (index, crop) in crops.iter().enumerate() {
        let resized_width = ((LINE_HEIGHT as f64 * ratio(crop)).ceil() as usize).clamp(1, width);
        crop.resize(resized_width, LINE_HEIGHT)
            .write_normalized(&mut data[index * item..(index + 1) * item], width);
    }
    Batch {
        data,
        count: crops.len(),
        width,
    }
}

/// Greedy CTC decoding of one `[steps, classes]` probability row.
///
/// Class 0 is the CTC blank; `charset[i]` is class `i + 1`. Repeats collapse
/// before blanks are removed. The score is the mean probability of the kept
/// steps, or 0 when none are kept.
pub fn decode(probabilities: &[f32], classes: usize, charset: &[String]) -> (String, f32) {
    let mut text = String::new();
    let mut kept = Vec::new();
    let mut previous = None;
    for step in probabilities.chunks_exact(classes) {
        let (class, probability) =
            step.iter()
                .copied()
                .enumerate()
                .fold(
                    (0, f32::MIN),
                    |best, next| if next.1 > best.1 { next } else { best },
                );
        if class != 0 && previous != Some(class) {
            if let Some(character) = charset.get(class - 1) {
                text.push_str(character);
            }
            kept.push(probability);
        }
        previous = Some(class);
    }
    let score = if kept.is_empty() {
        0.0
    } else {
        kept.iter().sum::<f32>() / kept.len() as f32
    };
    (text, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charset() -> Vec<String> {
        ["a", "b", " "].map(String::from).to_vec()
    }

    fn steps(classes: &[usize]) -> Vec<f32> {
        classes
            .iter()
            .flat_map(|&class| (0..4).map(move |i| if i == class { 0.8 } else { 0.05 }))
            .collect()
    }

    #[test]
    fn repeats_collapse_and_blanks_separate_repeated_characters() {
        let (text, score) = decode(&steps(&[1, 1, 0, 1, 2, 2, 0, 3, 2]), 4, &charset());
        assert_eq!(text, "aab b");
        assert!((score - 0.8).abs() < 1e-6);
    }

    #[test]
    fn all_blank_decodes_to_nothing_with_zero_score() {
        assert_eq!(
            decode(&steps(&[0, 0, 0]), 4, &charset()),
            (String::new(), 0.0)
        );
    }

    #[test]
    fn crops_share_the_widest_width_and_pad_with_zero() {
        let narrow = Bgr {
            width: 48,
            height: 48,
            data: vec![255; 48 * 48 * 3],
        };
        let wide = Bgr {
            width: 480,
            height: 48,
            data: vec![255; 480 * 48 * 3],
        };
        let batch = batch(&[&narrow, &wide]);
        assert_eq!((batch.count, batch.width), (2, 480));
        // First row of the narrow crop: 48 white columns, then padding.
        assert_eq!(batch.data[47], 1.0);
        assert_eq!(batch.data[48], 0.0);
    }

    #[test]
    fn short_batches_are_padded_to_the_minimum_ratio() {
        let crop = Bgr {
            width: 10,
            height: 10,
            data: vec![0; 300],
        };
        assert_eq!(batch(&[&crop]).width, 320);
    }
}
