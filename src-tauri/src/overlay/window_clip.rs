use crate::config::CaptureRegion;
use tauri::WebviewWindow;

const FRAME_BORDER_CSS_PX: f64 = 2.0;
const FRAME_RADIUS_CSS_PX: f64 = 8.0;
const SUBTITLE_RADIUS_CSS_PX: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ClipRect {
    fn from_region(region: CaptureRegion) -> Option<Self> {
        (region.width > 0 && region.height > 0).then_some(Self {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
        })
    }

    fn right(self) -> i32 {
        self.x.saturating_add(self.width)
    }

    fn bottom(self) -> i32 {
        self.y.saturating_add(self.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRing {
    pub outer: ClipRect,
    pub inner: Option<ClipRect>,
    pub outer_radius: i32,
    pub inner_radius: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundedClipRect {
    pub rect: ClipRect,
    pub radius: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipGeometry {
    pub frame_ring: Option<FrameRing>,
    pub solid_rects: Vec<RoundedClipRect>,
}

impl ClipGeometry {
    pub fn is_empty(&self) -> bool {
        self.frame_ring.is_none() && self.solid_rects.is_empty()
    }
}

fn normalized_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

fn scale_region(region: CaptureRegion, scale_factor: f64) -> Option<ClipRect> {
    ClipRect::from_region(region.scaled(scale_factor))
}

fn build_frame_ring(region: ClipRect, scale_factor: f64) -> FrameRing {
    let border = (FRAME_BORDER_CSS_PX * scale_factor).round().max(1.0) as i32;
    let radius = (FRAME_RADIUS_CSS_PX * scale_factor).round().max(0.0) as i32;
    let inner_width = region.width.saturating_sub(border.saturating_mul(2));
    let inner_height = region.height.saturating_sub(border.saturating_mul(2));

    FrameRing {
        outer: region,
        inner: (inner_width > 0 && inner_height > 0).then_some(ClipRect {
            x: region.x.saturating_add(border),
            y: region.y.saturating_add(border),
            width: inner_width,
            height: inner_height,
        }),
        outer_radius: radius,
        inner_radius: radius.saturating_sub(border),
    }
}

fn rounded_rect(rect: ClipRect, radius_css_px: f64, scale_factor: f64) -> RoundedClipRect {
    let radius = (radius_css_px * scale_factor).round().max(0.0) as i32;
    RoundedClipRect {
        rect,
        radius: radius.min(rect.width / 2).min(rect.height / 2),
    }
}

/// Build the device-pixel geometry used by the native overlay region.
///
/// The frame is only a thin ring. Resize handles, the settings button, the
/// subtitle, and other visible controls remain separate rectangles so a
/// transparency failure cannot turn the capture area into an opaque block.
pub fn build_clip_geometry(
    frame_region: Option<CaptureRegion>,
    subtitle_bounds: Option<CaptureRegion>,
    control_bounds: Option<&[CaptureRegion]>,
    control_radii: Option<&[f64]>,
    scale_factor: f64,
) -> ClipGeometry {
    let scale_factor = normalized_scale_factor(scale_factor);
    let frame_ring = frame_region
        .and_then(|region| scale_region(region, scale_factor))
        .map(|region| build_frame_ring(region, scale_factor));

    let mut solid_rects = Vec::new();
    if let Some(bounds) = subtitle_bounds.and_then(|bounds| scale_region(bounds, scale_factor)) {
        solid_rects.push(rounded_rect(bounds, SUBTITLE_RADIUS_CSS_PX, scale_factor));
    }
    if let Some(control_bounds) = control_bounds {
        solid_rects.extend(control_bounds.iter().copied().enumerate().filter_map(
            |(index, bounds)| {
                scale_region(bounds, scale_factor).map(|bounds| {
                    let radius = control_radii
                        .and_then(|radii| radii.get(index))
                        .copied()
                        .unwrap_or(0.0);
                    rounded_rect(bounds, radius, scale_factor)
                })
            },
        ));
    }

    ClipGeometry {
        frame_ring,
        solid_rects,
    }
}

pub fn apply_overlay_window_clip(
    window: &WebviewWindow,
    frame_region: Option<CaptureRegion>,
    subtitle_bounds: Option<CaptureRegion>,
    control_bounds: Option<Vec<CaptureRegion>>,
    control_radii: Option<Vec<f64>>,
    scale_factor: f64,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        apply_windows_clip(
            window,
            frame_region,
            subtitle_bounds,
            control_bounds.as_deref(),
            control_radii.as_deref(),
            scale_factor,
        )
    }

    #[cfg(not(windows))]
    {
        let _ = (
            window,
            frame_region,
            subtitle_bounds,
            control_bounds,
            control_radii,
            scale_factor,
        );
        Ok(())
    }
}

#[cfg(windows)]
fn apply_windows_clip(
    window: &WebviewWindow,
    frame_region: Option<CaptureRegion>,
    subtitle_bounds: Option<CaptureRegion>,
    control_bounds: Option<&[CaptureRegion]>,
    control_radii: Option<&[f64]>,
    scale_factor: f64,
) -> Result<(), String> {
    use raw_window_handle::HasWindowHandle;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, RGN_DIFF, RGN_OR,
    };

    let handle = window
        .window_handle()
        .map_err(|e| format!("Failed to get window handle: {}", e))?;
    let hwnd = match handle.as_raw() {
        raw_window_handle::RawWindowHandle::Win32(win32) => HWND(win32.hwnd.get() as *mut _),
        _ => return Err("Overlay window is not a Win32 window".to_string()),
    };

    let geometry = build_clip_geometry(
        frame_region,
        subtitle_bounds,
        control_bounds,
        control_radii,
        scale_factor,
    );

    unsafe {
        let mut region_to_set = None;

        if let Some(frame) = geometry.frame_ring {
            let outer = CreateRoundRectRgn(
                frame.outer.x,
                frame.outer.y,
                frame.outer.right(),
                frame.outer.bottom(),
                frame.outer_radius * 2,
                frame.outer_radius * 2,
            );
            if outer.is_invalid() {
                return Err("CreateRoundRectRgn (frame outer) failed".to_string());
            }

            if let Some(inner) = frame.inner {
                let inner_rgn = CreateRoundRectRgn(
                    inner.x,
                    inner.y,
                    inner.right(),
                    inner.bottom(),
                    frame.inner_radius * 2,
                    frame.inner_radius * 2,
                );
                if inner_rgn.is_invalid() {
                    let inner_rgn = CreateRectRgn(inner.x, inner.y, inner.right(), inner.bottom());
                    if inner_rgn.is_invalid() {
                        let _ = DeleteObject(outer.into());
                        return Err("CreateRectRgn (frame inner) failed".to_string());
                    }
                    let _ = CombineRgn(Some(outer), Some(outer), Some(inner_rgn), RGN_DIFF);
                    let _ = DeleteObject(inner_rgn.into());
                } else {
                    let _ = CombineRgn(Some(outer), Some(outer), Some(inner_rgn), RGN_DIFF);
                    let _ = DeleteObject(inner_rgn.into());
                }
            }

            region_to_set = Some(outer);
        }

        for rounded in geometry.solid_rects {
            let rect = rounded.rect;
            let rgn = if rounded.radius > 0 {
                CreateRoundRectRgn(
                    rect.x,
                    rect.y,
                    rect.right(),
                    rect.bottom(),
                    rounded.radius * 2,
                    rounded.radius * 2,
                )
            } else {
                CreateRectRgn(rect.x, rect.y, rect.right(), rect.bottom())
            };
            if rgn.is_invalid() {
                continue;
            }

            match region_to_set {
                Some(existing) => {
                    let _ = CombineRgn(Some(existing), Some(existing), Some(rgn), RGN_OR);
                    let _ = DeleteObject(rgn.into());
                }
                None => region_to_set = Some(rgn),
            }
        }

        // An empty region keeps a shown fullscreen window from falling back to
        // its default rectangular client area when no UI is visible.
        let region = region_to_set.unwrap_or_else(|| CreateRectRgn(0, 0, 0, 0));
        if region.is_invalid() {
            return Err("CreateRectRgn (empty overlay) failed".to_string());
        }

        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(region.into());
            return Err("SetWindowRgn failed".to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "window_clip_tests.rs"]
mod tests;
