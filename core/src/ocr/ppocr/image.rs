//! Packed BGR8 images and the few resampling operations PP-OCR needs.
//!
//! Resampling follows OpenCV's `INTER_LINEAR` pixel-centre convention, because
//! the reference pipeline the models were measured with uses it.

#[derive(Debug, Clone, PartialEq)]
pub struct Bgr {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Bgr {
    /// Drop alpha from a tightly packed BGRA8 frame.
    pub fn from_bgra(bytes: &[u8], width: usize, height: usize) -> Self {
        let data = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[blue, green, red, _]| [blue, green, red])
            .collect();
        Self {
            width,
            height,
            data,
        }
    }

    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let at = (y * self.width + x) * 3;
        [self.data[at], self.data[at + 1], self.data[at + 2]]
    }

    pub fn crop(&self, x: usize, y: usize, width: usize, height: usize) -> Self {
        let mut data = Vec::with_capacity(width * height * 3);
        for row in y..y + height {
            let start = (row * self.width + x) * 3;
            data.extend_from_slice(&self.data[start..start + width * 3]);
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// Rotate 90 degrees counter-clockwise, as `numpy.rot90` does.
    pub fn rotate_counter_clockwise(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for column in (0..self.width).rev() {
            for row in 0..self.height {
                data.extend_from_slice(&self.pixel(column, row));
            }
        }
        Self {
            width: self.height,
            height: self.width,
            data,
        }
    }

    pub fn resize(&self, width: usize, height: usize) -> Self {
        let columns = axis(self.width, width);
        let rows = axis(self.height, height);
        let mut data = Vec::with_capacity(width * height * 3);
        for &(y0, y1, fy) in &rows {
            for &(x0, x1, fx) in &columns {
                let (a, b) = (self.pixel(x0, y0), self.pixel(x1, y0));
                let (c, d) = (self.pixel(x0, y1), self.pixel(x1, y1));
                for channel in 0..3 {
                    let top = f32::from(a[channel]) * (1.0 - fx) + f32::from(b[channel]) * fx;
                    let bottom = f32::from(c[channel]) * (1.0 - fx) + f32::from(d[channel]) * fx;
                    data.push((top * (1.0 - fy) + bottom * fy).round() as u8);
                }
            }
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// Write the image as planar BGR scaled to -1..=1 into `out`, a
    /// `[3, out_height, out_width]` tensor slice. Columns beyond the image
    /// stay as they are, which callers use for zero padding.
    pub fn write_normalized(&self, out: &mut [f32], out_width: usize) {
        let plane = out.len() / 3;
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = self.pixel(x, y);
                for (channel, value) in pixel.into_iter().enumerate() {
                    out[channel * plane + y * out_width + x] = f32::from(value) / 127.5 - 1.0;
                }
            }
        }
    }
}

/// Source index pairs and weights for each destination index.
fn axis(source: usize, destination: usize) -> Vec<(usize, usize, f32)> {
    let scale = source as f32 / destination as f32;
    (0..destination)
        .map(|index| {
            let position = ((index as f32 + 0.5) * scale - 0.5).max(0.0);
            let low = (position as usize).min(source - 1);
            let high = (low + 1).min(source - 1);
            (low, high, position - low as f32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: usize, height: usize) -> Bgr {
        let data = (0..width * height)
            .flat_map(|i| {
                let v = (i * 10 % 256) as u8;
                [v, v / 2, 255 - v]
            })
            .collect();
        Bgr {
            width,
            height,
            data,
        }
    }

    #[test]
    fn resize_to_the_same_size_is_identity() {
        let image = gradient(7, 5);
        assert_eq!(image.resize(7, 5), image);
    }

    #[test]
    fn halving_averages_neighbouring_pixels() {
        let image = Bgr {
            width: 2,
            height: 1,
            data: vec![0, 0, 0, 200, 100, 50],
        };
        assert_eq!(image.resize(1, 1).data, vec![100, 50, 25]);
    }

    #[test]
    fn rotation_matches_numpy_rot90() {
        let image = gradient(3, 2);
        let rotated = image.rotate_counter_clockwise();
        assert_eq!((rotated.width, rotated.height), (2, 3));
        // numpy: rotated[i][j] = image[j][width - 1 - i]
        assert_eq!(rotated.pixel(0, 0), image.pixel(2, 0));
        assert_eq!(rotated.pixel(1, 2), image.pixel(0, 1));
    }

    #[test]
    fn crop_takes_the_requested_rectangle() {
        let image = gradient(4, 3);
        let cropped = image.crop(1, 1, 2, 2);
        assert_eq!((cropped.width, cropped.height), (2, 2));
        assert_eq!(cropped.pixel(0, 0), image.pixel(1, 1));
        assert_eq!(cropped.pixel(1, 1), image.pixel(2, 2));
    }

    #[test]
    fn normalization_maps_bytes_to_unit_range_and_leaves_padding() {
        let image = Bgr {
            width: 1,
            height: 1,
            data: vec![0, 255, 0],
        };
        let mut out = vec![9.0; 3 * 2];
        image.write_normalized(&mut out, 2);
        assert_eq!(out, vec![-1.0, 9.0, 1.0, 9.0, -1.0, 9.0]);
    }
}
