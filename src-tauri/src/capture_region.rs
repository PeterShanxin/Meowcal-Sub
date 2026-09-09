//! The capture region and its geometry.
//!
//! Split from `config` because the rectangle is persisted there but reasoned
//! about everywhere else: the selector, the capture loop, and the overlay all
//! scale it, clamp it to a monitor, and test it against bounds.

use serde::{Deserialize, Serialize};

/// A rectangular region on the screen
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureRegion {
    /// X coordinate of the top-left corner
    pub x: i32,
    /// Y coordinate of the top-left corner
    pub y: i32,
    /// Width of the region
    pub width: i32,
    /// Height of the region
    pub height: i32,
}

impl CaptureRegion {
    /// Create a new capture region
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if the region is valid (positive dimensions)
    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

    /// Get the area in pixels
    pub fn area(&self) -> i32 {
        self.width * self.height
    }

    /// Scale the region by a DPI scale factor (logical -> physical pixels)
    pub fn scaled(&self, scale: f64) -> Self {
        if (scale - 1.0).abs() < f64::EPSILON {
            return *self;
        }

        let scaled_x = (self.x as f64 * scale).round() as i32;
        let scaled_y = (self.y as f64 * scale).round() as i32;
        let scaled_width = (self.width as f64 * scale).round().max(1.0) as i32;
        let scaled_height = (self.height as f64 * scale).round().max(1.0) as i32;

        Self {
            x: scaled_x,
            y: scaled_y,
            width: scaled_width,
            height: scaled_height,
        }
    }

    /// Returns true if this region overlaps the origin-based bounds rectangle (0..width, 0..height).
    pub fn intersects_origin_bounds(&self, bounds_width: i32, bounds_height: i32) -> bool {
        if !self.is_valid() || bounds_width <= 0 || bounds_height <= 0 {
            return false;
        }

        let left = self.x as i64;
        let top = self.y as i64;
        let right = left + self.width as i64;
        let bottom = top + self.height as i64;

        let bounds_right = bounds_width as i64;
        let bounds_bottom = bounds_height as i64;

        let inter_left = left.max(0);
        let inter_top = top.max(0);
        let inter_right = right.min(bounds_right);
        let inter_bottom = bottom.min(bounds_bottom);

        inter_right > inter_left && inter_bottom > inter_top
    }

    /// Clamp this region to fit within origin-based bounds (0..width, 0..height).
    ///
    /// The clamping behavior preserves the current width/height whenever possible by shifting the
    /// region back into view. If the region is larger than the bounds, it will be capped.
    pub fn clamp_to_bounds(&self, bounds_width: i32, bounds_height: i32) -> Option<Self> {
        if !self.is_valid() || bounds_width <= 0 || bounds_height <= 0 {
            return None;
        }

        let mut width = self.width;
        let mut height = self.height;
        let mut x = self.x;
        let mut y = self.y;

        // Cap the region size to the available bounds.
        if width > bounds_width {
            width = bounds_width;
            x = 0;
        }
        if height > bounds_height {
            height = bounds_height;
            y = 0;
        }

        // Shift into bounds while preserving size.
        if x < 0 {
            x = 0;
        }
        if y < 0 {
            y = 0;
        }

        // Ensure right/bottom edge doesn't exceed bounds.
        if (x as i64) + (width as i64) > (bounds_width as i64) {
            x = bounds_width - width;
        }
        if (y as i64) + (height as i64) > (bounds_height as i64) {
            y = bounds_height - height;
        }

        // Final sanity check.
        if width <= 0 || height <= 0 || x < 0 || y < 0 {
            return None;
        }

        Some(Self {
            x,
            y,
            width,
            height,
        })
    }
}
