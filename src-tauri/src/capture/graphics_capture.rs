// =============================================================================
// GRAPHICS_CAPTURE.RS - Windows.Graphics.Capture Screen Capture
// =============================================================================
// This file implements screen capture using the Windows.Graphics.Capture API.
// Unlike the GDI-based capture in win32.rs, this API can capture:
// - Hardware-accelerated video content (YouTube, Netflix, etc.)
// - DirectX games
// - Any content rendered by the GPU
//
// Features:
// - Persistent capture session (no flashing border on each frame)
// - Border disabled (requires Windows 10 2004+)
// - Efficient frame grabbing from pre-allocated pool
// =============================================================================

use super::d3d;
use super::frame::{crop_region, read_texture_pixels};
use super::{CaptureError, CaptureResult};
use crate::capture_target::CaptureMonitor;
use crate::config::CaptureRegion;
use crate::sync_utils::lock_or_recover;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{info, warn};

use windows::core::{IInspectable, Interface};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

// =============================================================================
// PERSISTENT CAPTURE SESSION
// =============================================================================

/// A persistent screen capture session that can grab multiple frames efficiently.
///
/// This struct holds all the resources needed for capture and keeps them alive
/// across multiple frame grabs, avoiding the overhead of creating a new session
/// for each capture and eliminating the flashing border.
pub struct ScreenCaptureSession {
    monitor: CaptureMonitor,
    item: GraphicsCaptureItem,
    closed_token: i64,
    closed: Arc<AtomicBool>,
    d3d_device: ID3D11Device,
    d3d_context: ID3D11DeviceContext,
    frame_pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    frame_receiver: Receiver<Direct3D11CaptureFrame>,
    full_width: u32,
    full_height: u32,
    staging_texture: Option<ID3D11Texture2D>,
    last_full_buffer: Option<Vec<u8>>,
}

impl ScreenCaptureSession {
    /// Create a new persistent capture session for the selected monitor.
    ///
    /// This sets up all the D3D11 resources and starts the capture session.
    /// The border is disabled if supported (Windows 10 2004+).
    pub fn new(monitor: &CaptureMonitor) -> Result<Self, CaptureError> {
        info!("Creating persistent screen capture session...");

        // Check if Graphics Capture is supported
        if !is_graphics_capture_supported() {
            return Err(CaptureError::GraphicsCaptureError(
                "Windows.Graphics.Capture is not supported. Requires Windows 10 1803+.".to_string(),
            ));
        }

        // Create D3D device
        let d3d_device = d3d::create_d3d_device()
            .map_err(|e| CaptureError::D3DError(format!("Failed to create D3D device: {}", e)))?;

        let d3d_context = unsafe {
            d3d_device
                .GetImmediateContext()
                .map_err(|e| CaptureError::D3DError(format!("Failed to get D3D context: {}", e)))?
        };

        let monitor_handle =
            crate::display_monitor::handle(monitor).map_err(|_| CaptureError::DisplayChanged)?;

        // Create capture item for the monitor
        let item = create_capture_item_for_monitor(monitor_handle)?;
        let closed = Arc::new(AtomicBool::new(false));
        let was_closed = closed.clone();
        let closed_token = item
            .Closed(
                &TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
                    was_closed.store(true, Ordering::Release);
                    Ok(())
                }),
            )
            .map_err(|e| CaptureError::GraphicsCaptureError(e.to_string()))?;

        // Get screen size
        let item_size = item.Size().map_err(|e| {
            CaptureError::GraphicsCaptureError(format!("Failed to get item size: {}", e))
        })?;

        if item_size.Width != monitor.bounds.width || item_size.Height != monitor.bounds.height {
            return Err(CaptureError::DisplayChanged);
        }
        let full_width = item_size.Width as u32;
        let full_height = item_size.Height as u32;
        info!("Capture size: {}x{}", full_width, full_height);

        // Create the WinRT device wrapper
        let device = d3d::create_direct3d_device(&d3d_device).map_err(|e| {
            CaptureError::D3DError(format!("Failed to create Direct3D device: {}", e))
        })?;

        // Create a frame pool with 2 buffers for double-buffering
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2, // Number of frames in pool (double-buffering)
            item_size,
        )
        .map_err(|e| {
            CaptureError::GraphicsCaptureError(format!("Failed to create frame pool: {}", e))
        })?;

        // Create a capture session
        let session = frame_pool.CreateCaptureSession(&item).map_err(|e| {
            CaptureError::GraphicsCaptureError(format!("Failed to create capture session: {}", e))
        })?;

        // Try to disable the border (Windows 10 2004+ / Build 19041+)
        // This removes the yellow/green border that appears when capturing
        match session.SetIsBorderRequired(false) {
            Ok(_) => info!("✅ Screen capture border disabled"),
            Err(e) => warn!(
                "⚠️ Could not disable capture border (requires Windows 10 2004+): {}",
                e
            ),
        }

        // Set up a channel to receive captured frames
        let (sender, receiver) = channel();

        // Subscribe to frame arrived events
        frame_pool
            .FrameArrived(
                &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new({
                    let sender = sender.clone();
                    move |frame_pool, _| {
                        if let Some(frame_pool) = frame_pool.as_ref() {
                            if let Ok(frame) = frame_pool.TryGetNextFrame() {
                                // Send frame, ignore if receiver is gone
                                let _ = sender.send(frame);
                            }
                        }
                        Ok(())
                    }
                }),
            )
            .map_err(|e| {
                CaptureError::GraphicsCaptureError(format!(
                    "Failed to subscribe to frame events: {}",
                    e
                ))
            })?;

        // Start capturing
        session.StartCapture().map_err(|e| {
            CaptureError::GraphicsCaptureError(format!("Failed to start capture: {}", e))
        })?;

        info!("✅ Persistent capture session started");

        Ok(Self {
            monitor: monitor.clone(),
            item,
            closed_token,
            closed,
            d3d_device,
            d3d_context,
            frame_pool,
            session,
            frame_receiver: receiver,
            full_width,
            full_height,
            staging_texture: None,
            last_full_buffer: None,
        })
    }

    /// Ensure we have a captured full-screen frame in memory.
    ///
    /// If no new frame arrives, reuses the last captured buffer. This avoids
    /// treating "no changes on screen" as an error, and keeps region cropping
    /// responsive when the user moves/resizes the capture box.
    fn ensure_full_frame(&mut self) -> Result<(), CaptureError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(CaptureError::DisplayChanged);
        }
        // Drain all available frames to get the latest one
        let mut frame = None;
        while let Ok(f) = self.frame_receiver.try_recv() {
            frame = Some(f);
        }

        let frame = match frame {
            Some(f) => f,
            None => {
                // If we already have a buffer, it's fine to reuse it.
                if self.last_full_buffer.is_some() {
                    return Ok(());
                }

                // Otherwise, wait a bit longer for the very first frame.
                self.frame_receiver
                    .recv_timeout(std::time::Duration::from_millis(1500))
                    .map_err(|_| {
                        CaptureError::GraphicsCaptureError(
                            "Timeout waiting for first frame".to_string(),
                        )
                    })?
            }
        };

        let size = frame
            .ContentSize()
            .map_err(|e| CaptureError::GraphicsCaptureError(e.to_string()))?;
        if size.Width != self.full_width as i32 || size.Height != self.full_height as i32 {
            self.last_full_buffer = None;
            return Err(CaptureError::DisplayChanged);
        }

        if self.closed.load(Ordering::Acquire) {
            return Err(CaptureError::DisplayChanged);
        }
        // Get the surface from the frame
        let surface = frame.Surface().map_err(|e| {
            CaptureError::GraphicsCaptureError(format!("Failed to get frame surface: {}", e))
        })?;

        // Get the D3D11 texture from the surface
        let source_texture: ID3D11Texture2D = d3d::get_d3d_interface_from_object(&surface)
            .map_err(|e| {
                CaptureError::D3DError(format!("Failed to get texture from surface: {}", e))
            })?;

        // Get or create staging texture
        let staging_texture = self.get_or_create_staging_texture(&source_texture)?;

        // Copy from GPU texture to staging texture
        unsafe {
            self.d3d_context.CopyResource(
                Some(
                    &staging_texture
                        .cast()
                        .map_err(|e| CaptureError::D3DError(e.to_string()))?,
                ),
                Some(
                    &source_texture
                        .cast()
                        .map_err(|e| CaptureError::D3DError(e.to_string()))?,
                ),
            );
        }

        // Read pixels from staging texture
        let buffer = read_texture_pixels(
            &self.d3d_context,
            &staging_texture,
            self.full_width,
            self.full_height,
        )?;
        self.last_full_buffer = Some(buffer);
        Ok(())
    }

    /// Get or create a staging texture for CPU readback.
    fn get_or_create_staging_texture(
        &mut self,
        source: &ID3D11Texture2D,
    ) -> Result<ID3D11Texture2D, CaptureError> {
        if let Some(ref texture) = self.staging_texture {
            return Ok(texture.clone());
        }

        // Get source texture description
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { source.GetDesc(&mut desc) };

        // Modify for staging (CPU read access)
        desc.BindFlags = 0;
        desc.MiscFlags = 0;
        desc.Usage = D3D11_USAGE_STAGING;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;

        let texture = unsafe {
            let mut texture = None;
            self.d3d_device
                .CreateTexture2D(&desc, None, Some(&mut texture))
                .map_err(|e| {
                    CaptureError::D3DError(format!("Failed to create staging texture: {}", e))
                })?;
            texture.ok_or_else(|| {
                CaptureError::D3DError("CreateTexture2D returned no texture".into())
            })?
        };

        self.staging_texture = Some(texture.clone());
        Ok(texture)
    }

    /// Capture a specific region of the screen.
    ///
    /// This grabs the full screen and then crops to the requested region.
    pub fn capture_region(
        &mut self,
        region: &CaptureRegion,
    ) -> Result<CaptureResult, CaptureError> {
        // Validate the region
        if !region.is_valid() {
            return Err(CaptureError::InvalidRegion(format!(
                "Invalid dimensions: {}x{}",
                region.width, region.height
            )));
        }

        crate::display_monitor::handle(&self.monitor).map_err(|_| CaptureError::DisplayChanged)?;
        self.ensure_full_frame()?;
        let full_buffer = self.last_full_buffer.as_deref().ok_or_else(|| {
            CaptureError::GraphicsCaptureError("No captured frame available".to_string())
        })?;

        // Crop to region
        let cropped = crop_region(full_buffer, self.full_width, self.full_height, region)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(CaptureError::DisplayChanged);
        }

        Ok(CaptureResult::new(
            cropped,
            region.width as u32,
            region.height as u32,
        ))
    }

    /// Close the capture session and release resources.
    pub fn close(&self) {
        info!("Closing capture session...");
        let _ = self.session.Close();
        let _ = self.frame_pool.Close();
        let _ = self.item.RemoveClosed(self.closed_token);
    }
}

impl Drop for ScreenCaptureSession {
    fn drop(&mut self) {
        self.close();
    }
}

// =============================================================================
// GLOBAL SESSION MANAGEMENT
// =============================================================================

lazy_static::lazy_static! {
    static ref CAPTURE_SESSION: Mutex<Option<ScreenCaptureSession>> = Mutex::new(None);
}

/// Initialize the global capture session.
/// Call this once when starting translation.
pub fn init_capture_session(monitor: &CaptureMonitor) -> Result<(), CaptureError> {
    let mut session = lock_or_recover(&CAPTURE_SESSION);
    if session
        .as_ref()
        .is_none_or(|session| &session.monitor != monitor)
    {
        *session = Some(ScreenCaptureSession::new(monitor)?);
    }
    Ok(())
}

/// Close the global capture session.
/// Call this when stopping translation.
pub fn close_capture_session() {
    let mut session = lock_or_recover(&CAPTURE_SESSION);
    let _ = session.take();
}

/// Capture a region using the persistent session.
pub fn capture_with_session(
    monitor: &CaptureMonitor,
    region: &CaptureRegion,
) -> Result<CaptureResult, CaptureError> {
    let mut session = lock_or_recover(&CAPTURE_SESSION);
    match session.as_mut() {
        Some(s) if &s.monitor == monitor => s.capture_region(region),
        Some(_) => Err(CaptureError::GraphicsCaptureError(
            "Capture session belongs to another display".into(),
        )),
        None => Err(CaptureError::GraphicsCaptureError(
            "Capture session not initialized. Call init_capture_session() first.".to_string(),
        )),
    }
}

// =============================================================================
// STANDALONE FUNCTIONS (original API, kept for compatibility)
// =============================================================================

/// Check if the Graphics Capture API is supported on this system
pub fn is_graphics_capture_supported() -> bool {
    GraphicsCaptureSession::IsSupported().unwrap_or(false)
}

/// Create a GraphicsCaptureItem for the selected monitor
fn create_capture_item_for_monitor(
    monitor_handle: HMONITOR,
) -> Result<GraphicsCaptureItem, CaptureError> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|e| {
            CaptureError::GraphicsCaptureError(format!("Failed to get capture interop: {}", e))
        })?;

    unsafe {
        interop.CreateForMonitor(monitor_handle).map_err(|e| {
            CaptureError::GraphicsCaptureError(format!(
                "Failed to create capture item for monitor: {}",
                e
            ))
        })
    }
}

/// Capture a region using a one-shot session (original API, creates new session each time)
/// This is kept for compatibility but the persistent session is preferred.
pub fn capture_region_graphics(
    monitor: &CaptureMonitor,
    region: &CaptureRegion,
) -> Result<CaptureResult, CaptureError> {
    // Use the persistent session if available
    let session = lock_or_recover(&CAPTURE_SESSION);
    if session
        .as_ref()
        .is_some_and(|session| &session.monitor == monitor)
    {
        drop(session); // Release lock
        return capture_with_session(monitor, region);
    }
    drop(session);

    // Otherwise create a one-shot session
    let mut session = ScreenCaptureSession::new(monitor)?;
    session.capture_region(region)
}

// =============================================================================
// TESTS
// =============================================================================
