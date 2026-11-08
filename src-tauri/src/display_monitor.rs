//! Windows monitor discovery. Tauri supplies physical bounds and per-monitor
//! DPI; the monitor device interface identifies the screen across restarts.

use crate::capture_target::{CaptureMonitor, TargetError};
use crate::config::CaptureRegion;
use tauri::{AppHandle, Manager, Monitor, WebviewWindow};
use windows::core::PCWSTR;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, GetMonitorInfoW, MonitorFromRect, DISPLAY_DEVICEW, HMONITOR, MONITORINFO,
    MONITORINFOEXW, MONITOR_DEFAULTTONULL,
};
use windows::Win32::UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME;

fn describe(monitor: &Monitor) -> Result<CaptureMonitor, String> {
    let position = monitor.position();
    let size = monitor.size();
    let bounds = CaptureRegion::new(
        position.x,
        position.y,
        size.width as i32,
        size.height as i32,
    );
    let (_, device_id) = identity_at(bounds)?;
    Ok(CaptureMonitor {
        device_id,
        bounds,
        scale_factor: monitor.scale_factor(),
    })
}

pub fn available(app: &AppHandle) -> Result<Vec<CaptureMonitor>, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window
        .available_monitors()
        .map_err(|e| e.to_string())?
        .iter()
        .map(describe)
        .collect()
}

/// Windows chooses the largest intersection when the main window straddles
/// screens, and its restored rectangle when the window is minimized.
pub fn current(app: &AppHandle) -> Result<CaptureMonitor, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| TargetError::DisplayChanged.to_string())?;
    describe(&monitor)
}

pub fn require_present(app: &AppHandle, target: &CaptureMonitor) -> Result<(), String> {
    if target.is_present(&available(app)?) {
        Ok(())
    } else {
        Err(TargetError::DisplayChanged.to_string())
    }
}

/// Windows or a window manager can move a popup independently of Home.
pub fn require_overlay(app: &AppHandle, target: &CaptureMonitor) -> Result<(), String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or("Overlay window not found")?;
    let hwnd = windows::Win32::Foundation::HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    let mut rect = RECT::default();
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect) }
        .map_err(|e| e.to_string())?;
    let bounds = CaptureRegion::new(
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    );
    if bounds != target.bounds {
        return Err(TargetError::DisplayChanged.to_string());
    }
    Ok(())
}

/// Resolve a fresh handle. HMONITOR values can be invalidated or reused after
/// hotplug; neither a saved handle nor a display index is an identity.
pub fn handle(target: &CaptureMonitor) -> Result<HMONITOR, String> {
    let (handle, id) = identity_at(target.bounds)?;
    if id != target.device_id {
        return Err(TargetError::DisplayChanged.to_string());
    }
    Ok(handle)
}

fn identity_at(bounds: CaptureRegion) -> Result<(HMONITOR, String), String> {
    let right = bounds
        .x
        .checked_add(bounds.width)
        .ok_or("Invalid display bounds")?;
    let bottom = bounds
        .y
        .checked_add(bounds.height)
        .ok_or("Invalid display bounds")?;
    let rect = RECT {
        left: bounds.x,
        top: bounds.y,
        right,
        bottom,
    };
    let handle = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) };
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(handle, &mut info as *mut _ as *mut MONITORINFO) }.as_bool()
        || info.monitorInfo.rcMonitor != rect
    {
        return Err(TargetError::DisplayChanged.to_string());
    }
    let mut device = DISPLAY_DEVICEW {
        cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
        ..Default::default()
    };
    if !unsafe {
        EnumDisplayDevicesW(
            PCWSTR(info.szDevice.as_ptr()),
            0,
            &mut device,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
    }
    .as_bool()
    {
        return Err("Cannot identify the display. Reconnect the screen and try again.".into());
    }
    let length = device
        .DeviceID
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(device.DeviceID.len());
    let id = String::from_utf16_lossy(&device.DeviceID[..length]);
    if id.is_empty() {
        return Err("The display has no device identity.".into());
    }
    Ok((handle, id))
}

pub fn place(window: &WebviewWindow, target: &CaptureMonitor) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
    handle(target)?;
    let bounds = target.bounds;
    unsafe {
        SetWindowPos(
            windows::Win32::Foundation::HWND(window.hwnd().map_err(|e| e.to_string())?.0),
            None,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    }
    .map_err(|e| e.to_string())
}
