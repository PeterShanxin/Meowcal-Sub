// =============================================================================
// WINDOW ALPHA - translucency the compositor will actually honour
// =============================================================================
// WebView2 transparency does not hold on this platform. A clipped but unpainted
// area of the overlay renders as a solid white block, which means the webview's
// backing is opaque, which in turn means every `rgba()` in overlay.css
// composites against white instead of against the video. That is why a subtitle
// plate written as 72% black arrived on screen as flat grey.
//
// So the translucency is moved off the web page and onto the window: a layered
// window with a uniform alpha is blended by DWM against whatever is behind it,
// no webview cooperation required.
//
// The cost is that the alpha is uniform - frame, handles, gear, plate and
// subtitle text all get it. That is the trade this file is making: a plate you
// can see video through, with text at the same alpha over it, beats an opaque
// grey slab. Per-element alpha would need UpdateLayeredWindow, which cannot
// host a live WebView2 child.
// =============================================================================

/// Window opacity, 0 = invisible, 255 = opaque.
///
/// Chosen so the plate reads as a translucent subtitle bar rather than a
/// window someone forgot to finish: at 200 roughly a fifth of the video shows
/// through the plate, and white text over a near-black plate stays legible.
/// The plate itself is a solid colour in overlay.css - stacking a CSS alpha on
/// top of this one would only dilute the text's own contrast.
pub const OVERLAY_ALPHA: u8 = 200;

/// Make the overlay window translucent.
///
/// Must be re-applied after every click-through change: `set_ignore_cursor_events`
/// owns `WS_EX_LAYERED` too, and turning click-through off strips the style,
/// taking the alpha with it. The overlay toggles click-through several times a
/// second while the cursor moves, so a one-shot call at startup would survive
/// only until the viewer moved the mouse.
#[cfg(windows)]
pub fn apply(window: &tauri::WebviewWindow) -> Result<(), String> {
    use raw_window_handle::HasWindowHandle;
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE, LWA_ALPHA,
        WS_EX_LAYERED,
    };

    let handle = window
        .window_handle()
        .map_err(|e| format!("Failed to get window handle: {}", e))?;

    let raw_window_handle::RawWindowHandle::Win32(win32_handle) = handle.as_raw() else {
        return Err("Window handle is not Win32".to_string());
    };

    // SAFETY: the HWND comes from Tauri's own window handle and is valid for
    // the lifetime of the borrow.
    unsafe {
        let hwnd = HWND(win32_handle.hwnd.get() as *mut _);
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let layered = ex_style | WS_EX_LAYERED.0 as isize;
        if layered != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, layered);
        }

        // Colour key is unused (LWA_ALPHA only), so it is left at black.
        SetLayeredWindowAttributes(hwnd, COLORREF(0), OVERLAY_ALPHA, LWA_ALPHA)
            .map_err(|e| format!("SetLayeredWindowAttributes failed: {}", e))?;
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn apply(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::OVERLAY_ALPHA;

    #[test]
    fn alpha_is_translucent_without_hiding_the_subtitles() {
        assert!(
            (128..=224).contains(&OVERLAY_ALPHA),
            "an overlay at {} is either opaque or unreadable",
            OVERLAY_ALPHA
        );
    }

    // The alpha lives in WS_EX_LAYERED, which set_ignore_cursor_events also
    // owns, so it is lost every time click-through is turned off - several
    // times a second while the cursor moves. Losing this call does not fail
    // any behavioural test; the overlay just quietly goes opaque again.
    #[test]
    fn click_through_command_reapplies_the_alpha() {
        let source = include_str!("commands.rs");
        let toggle = source
            .split_once("set_ignore_cursor_events")
            .expect("click-through command calls set_ignore_cursor_events")
            .1;
        assert!(
            toggle.contains("window_alpha::apply"),
            "set_overlay_click_through must restore the window alpha"
        );
    }
}
