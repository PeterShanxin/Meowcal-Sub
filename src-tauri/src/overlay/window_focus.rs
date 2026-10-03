//! Native keyboard access without activating the overlay during show or hover.
//!
//! Tao clears its initial `focus: false` marker after window creation. Its later
//! show and cursor-style changes use SW_SHOW, which activates a focusable window.
//! Keep Tao's `focusable: false` policy for those operations, then remove the
//! native no-activate flag so an explicit click or Alt+Tab can focus the WebView.
//! Reapply after every cursor-style change, just like the window alpha.

#[cfg(windows)]
pub fn allow_interaction(window: &tauri::WebviewWindow) -> Result<(), String> {
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            if let Err(error) = apply(&target) {
                tracing::warn!("Failed to enable overlay keyboard interaction: {error}");
            }
        })
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn apply(window: &tauri::WebviewWindow) -> Result<(), String> {
    use raw_window_handle::HasWindowHandle;
    use windows::Win32::Foundation::HWND;

    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("Window handle is not Win32".to_string());
    };

    // SAFETY: the HWND is borrowed from the live Tauri window. Only the
    // activation policy bit changes; layering and click-through are preserved.
    unsafe {
        allow_hwnd_interaction(HWND(handle.hwnd.get() as *mut _));
    }
    Ok(())
}

#[cfg(windows)]
unsafe fn allow_hwnd_interaction(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let interactive = style & !(WS_EX_NOACTIVATE.0 as isize);
    if style != interactive {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, interactive);
    }
}

#[cfg(not(windows))]
pub fn allow_interaction(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::allow_hwnd_interaction;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };

    #[test]
    fn keyboard_policy_preserves_native_layering_and_click_through() {
        // SAFETY: this test owns the hidden window and destroys it before return.
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
                w!("STATIC"),
                w!("Overlay focus test"),
                WS_POPUP,
                0,
                0,
                100,
                100,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let before = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            allow_hwnd_interaction(hwnd);
            let after = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            DestroyWindow(hwnd).unwrap();
            assert_ne!(before & WS_EX_NOACTIVATE.0 as isize, 0);
            assert_eq!(after & WS_EX_NOACTIVATE.0 as isize, 0);
            assert_eq!(after, before & !(WS_EX_NOACTIVATE.0 as isize));
        }
    }
}
