// Wait for Tauri to be ready
document.addEventListener('DOMContentLoaded', () => {
    console.log('🎬 Overlay loaded!');
    initOverlay();
});

const { getTranslationPresentation } = window.TranslationDisplay;
const { clearSubtitleHint, setSubtitleHint, updateSubtitleHint } = window.OverlaySubtitleHint;
const { appendClipSurface } = window.OverlayWindowClip;
const { resolveSubtitleSurface } = window.OverlaySubtitleSurface;
const { setupSettingsMenu } = window.OverlaySettingsMenu;

// =============================================================================
// GLOBAL STATE
// =============================================================================

const overlayState = {
    region: null,           // { x, y, width, height }
    isVisible: false,       // Whether translation is active
    isOverlayActive: false, // Whether user is interacting with overlay
    currentText: '',
    debugMode: true,
    // Overlay appearance settings
    fontSize: 24,
    fontFamily: 'Segoe UI',
    textColor: '#FFFFFF',
    backgroundColor: 'rgba(0, 0, 0, 0.75)',
    showDiagnostics: false, // Whether to show the diagnostics panel
    lastPipelinePosition: null,
    settingsOpen: false,
    isDragging: false,
    isResizing: false,
    resizeHandle: null,     // Which handle is being dragged
    dragStart: { x: 0, y: 0, regionX: 0, regionY: 0, regionW: 0, regionH: 0 },
    scaleFactor: 1,
    scaleFactorUpdatedAt: 0,
    isClickThrough: null,
    isHoveringInteractive: false,
    forceInteractiveUntil: 0,
    clickThroughEnabled: true,
};

// Timers
let clickThroughMonitor = null;
let clickThroughBusy = false;
let fadeTimer = null;

// Fade duration for "start/stop translation" show/hide transitions.
// Keep this short to avoid delaying stop/start responsiveness.
// IMPORTANT: Must match OVERLAY_HIDE_FADE_MS in src-tauri/src/commands.rs
const OVERLAY_VISIBILITY_FADE_MS = 220;

function syncFrameScaleTokens(scaleFactor) {
    // Keep the frame border aligned to device pixels at fractional DPI (e.g. 125%),
    // which reduces jaggy/blurred edges compared to a fixed CSS pixel border.
    const sf = (typeof scaleFactor === 'number' && scaleFactor > 0) ? scaleFactor : 1;
    const borderPx = Math.max(1, Math.round(2 * sf));   // physical px
    const radiusPx = Math.max(0, Math.round(8 * sf));   // physical px
    const borderCss = borderPx / sf;
    const radiusCss = radiusPx / sf;

    document.documentElement.style.setProperty('--frame-border', `${borderCss}px`);
    document.documentElement.style.setProperty('--frame-radius', `${radiusCss}px`);
}

// =============================================================================
// INTERACTION MANAGEMENT (Smart click-through)
// =============================================================================

// The overlay toggles click-through so users can interact with other apps.
// It becomes interactive when the cursor is over the capture frame or subtitles.

function setOverlayActive(active) {
    overlayState.isOverlayActive = active;
}

function updateInteractionClasses() {
    const isInteracting = overlayState.isDragging || overlayState.isResizing;
    const captureFrame = document.getElementById('capture-frame');
    const subtitleContainer = document.getElementById('subtitle-container');

    if (captureFrame) {
        captureFrame.classList.toggle('dragging', isInteracting);
    }
    if (subtitleContainer) {
        subtitleContainer.classList.toggle('dragging', isInteracting);
    }
}

async function refreshScaleFactor() {
    if (!window.__TAURI__?.window?.getCurrentWindow) return overlayState.scaleFactor || 1;

    try {
        const currentWindow = window.__TAURI__.window.getCurrentWindow();
        const scaleFactor = await currentWindow.scaleFactor();
        overlayState.scaleFactor = scaleFactor;
        overlayState.scaleFactorUpdatedAt = Date.now();
        syncFrameScaleTokens(scaleFactor);
        return scaleFactor;
    } catch (e) {
        console.warn('Failed to read scale factor:', e);
        return overlayState.scaleFactor || 1;
    }
}

async function getScaleFactor() {
    const now = Date.now();
    if (overlayState.scaleFactor && now - overlayState.scaleFactorUpdatedAt < 2000) {
        return overlayState.scaleFactor;
    }
    return refreshScaleFactor();
}

async function setOverlayClickThrough(ignore) {
    if (!overlayState.clickThroughEnabled) return;
    if (overlayState.isClickThrough === ignore) return;
    if (!window.__TAURI__?.core?.invoke) return;

    try {
        await window.__TAURI__.core.invoke('set_overlay_click_through', { ignore });
        overlayState.isClickThrough = ignore;
    } catch (e) {
        console.warn('Failed to set overlay click-through:', e);
        overlayState.clickThroughEnabled = false;
    }
}

function cancelClickThrough() {
    overlayState.forceInteractiveUntil = Date.now() + 500;
    setOverlayClickThrough(false);
    setHoveringState(true);
}

function setHoveringState(isHovering) {
    if (overlayState.isHoveringInteractive === isHovering) return;
    overlayState.isHoveringInteractive = isHovering;

    if (isHovering) {
        setOverlayActive(true);
        showCaptureFrame();
    } else {
        setOverlayActive(false);
        scheduleFadeOut();
    }
}

function rectToPhysicalBounds(rect, scaleFactor) {
    const offsetX = window.screenX || 0;
    const offsetY = window.screenY || 0;

    return {
        left: (rect.left + offsetX) * scaleFactor,
        top: (rect.top + offsetY) * scaleFactor,
        right: (rect.right + offsetX) * scaleFactor,
        bottom: (rect.bottom + offsetY) * scaleFactor,
    };
}

function getInteractiveBoundsPhysical(scaleFactor) {
    const bounds = [];

    if (overlayState.region) {
        // Increase padding to cover the settings button (28px button at 8px from edge)
        const handlePadding = 40;
        bounds.push({
            left: (overlayState.region.x - handlePadding) * scaleFactor,
            top: (overlayState.region.y - handlePadding) * scaleFactor,
            right: (overlayState.region.x + overlayState.region.width + handlePadding) * scaleFactor,
            bottom: (overlayState.region.y + overlayState.region.height + handlePadding) * scaleFactor,
        });
    }

    const subtitleContainer = document.getElementById('subtitle-container');
    if (subtitleContainer && !subtitleContainer.classList.contains('hidden')) {
        const rect = subtitleContainer.getBoundingClientRect();
        bounds.push(rectToPhysicalBounds(rect, scaleFactor));
    }

    const settingsMenu = document.getElementById('settings-menu');
    if (settingsMenu && settingsMenu.classList.contains('visible')) {
        const rect = settingsMenu.getBoundingClientRect();
        bounds.push(rectToPhysicalBounds(rect, scaleFactor));
    }

    return bounds;
}

function pointInBounds(point, bounds) {
    return point.x >= bounds.left &&
        point.x <= bounds.right &&
        point.y >= bounds.top &&
        point.y <= bounds.bottom;
}

async function updateClickThroughState() {
    if (clickThroughBusy || !overlayState.isVisible) return;
    if (!window.__TAURI__?.window?.cursorPosition) return;

    clickThroughBusy = true;
    try {
        if (!overlayState.region) {
            await setOverlayClickThrough(true);
            return;
        }

        if (overlayState.isDragging || overlayState.isResizing || overlayState.settingsOpen) {
            setHoveringState(true);
            await setOverlayClickThrough(false);
            return;
        }

        const scaleFactor = await getScaleFactor();
        const cursor = await window.__TAURI__.window.cursorPosition();
        const bounds = getInteractiveBoundsPhysical(scaleFactor);
        const isForceInteractive = overlayState.forceInteractiveUntil > Date.now();
        const isHovering = isForceInteractive || bounds.some((rect) => pointInBounds(cursor, rect));

        setHoveringState(isHovering);
        await setOverlayClickThrough(!isHovering);
    } catch (e) {
        console.warn('Failed to update click-through state:', e);
    } finally {
        clickThroughBusy = false;
    }
}

function startClickThroughMonitor() {
    if (clickThroughMonitor) return;
    clickThroughMonitor = setInterval(updateClickThroughState, 150);
}

function stopClickThroughMonitor() {
    if (!clickThroughMonitor) return;
    clearInterval(clickThroughMonitor);
    clickThroughMonitor = null;
}

// =============================================================================
// CAPTURE FRAME FADE MANAGEMENT
// =============================================================================

function scheduleFadeOut() {
    if (fadeTimer) clearTimeout(fadeTimer);

    const captureFrame = document.getElementById('capture-frame');
    if (!captureFrame) return;

    // Fade out after 4 seconds of no interaction
    fadeTimer = setTimeout(() => {
        // The settings popup is anchored to the gear inside the frame, so fading
        // the frame while it is open would strand the popup with no way to close it.
        if (overlayState.settingsOpen) return;
        if (!overlayState.isOverlayActive && !overlayState.isDragging && !overlayState.isResizing) {
            captureFrame.classList.add('faded');
            scheduleWindowClipUpdate();
        }
    }, 4000);
}

function showCaptureFrame() {
    if (fadeTimer) clearTimeout(fadeTimer);

    const captureFrame = document.getElementById('capture-frame');
    if (captureFrame) {
        captureFrame.classList.remove('faded');
        scheduleWindowClipUpdate();
    }
}

// =============================================================================
// INITIALIZATION
// =============================================================================

async function initOverlay() {
    console.log('🔧 Initializing overlay...');

    // Force transparent background via WebView2 API (workaround for Tauri 2.0 transparency issues)
    // On Windows 8+, alpha=0 creates true transparency
    try {
        const currentWebview = window.__TAURI__.webview.getCurrentWebview();
        await currentWebview.setBackgroundColor([0, 0, 0, 0]);
        console.log('✅ Set overlay webview background to transparent');
    } catch (e) {
        console.warn('Could not set transparent background via webview API:', e);
        // Fallback: try window API (WebviewWindow combines window + webview)
        try {
            const currentWindow = window.__TAURI__.window.getCurrentWindow();
            await currentWindow.setBackgroundColor([0, 0, 0, 0]);
            console.log('✅ Set overlay window background to transparent (fallback)');
        } catch (e2) {
            console.warn('Could not set transparent background:', e2);
        }
    }

    const captureFrame = document.getElementById('capture-frame');
    const subtitleContainer = document.getElementById('subtitle-container');
    const subtitleHint = document.getElementById('subtitle-hint');
    const subtitleHintText = document.getElementById('subtitle-hint-text');
    const subtitleText = document.getElementById('subtitle-text');
    const debugRegion = document.getElementById('debug-region');
    const debugStatus = document.getElementById('debug-status');
    const settingsButton = document.getElementById('settings-button');
    const settingsMenu = document.getElementById('settings-menu');

    if (debugStatus) debugStatus.textContent = 'Status: setting up...';

    await refreshScaleFactor();

    // Load initial font size
    await loadOverlaySettings();

    // Set up resize handles
    setupResizeHandles(captureFrame);

    // Set up drag to reposition
    setupDragToReposition(captureFrame);

    // Set up hover detection for the entire interactive area
    setupHoverDetection(captureFrame, subtitleContainer);

    // Set up settings button
    setupSettingsButton(settingsButton, settingsMenu, subtitleText, subtitleContainer);

    // Set up event listeners
    await setupEventListeners({
        captureFrame,
        subtitleContainer,
        subtitleHint,
        subtitleHintText,
        subtitleText,
        debugRegion,
        debugStatus,
    });

    // Fetch initial state
    try {
        const region = await window.__TAURI__.core.invoke('get_capture_region');
        if (region) {
            console.log('📍 Initial region:', region);
            overlayState.region = region;
            updateCaptureFrame(captureFrame, region);
            updateSubtitlePosition(subtitleContainer, region);
            if (debugRegion) {
                debugRegion.textContent = `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
            }
        }
    } catch (e) {
        console.error('Failed to get initial region:', e);
    }

    if (debugStatus) debugStatus.textContent = 'Status: ready';
    console.log('✅ Overlay initialized');
}

// =============================================================================
// HOVER DETECTION - Controls click-through and fade
// =============================================================================

function setupHoverDetection(captureFrame, subtitleContainer) {
    // Track when user is hovering over interactive elements
    // This controls the fade behavior of the capture frame

    const onEnterInteractiveArea = () => {
        setOverlayActive(true);
        showCaptureFrame();
        scheduleWindowClipUpdate();
    };

    const onLeaveInteractiveArea = () => {
        if (!overlayState.isDragging && !overlayState.isResizing && !overlayState.settingsOpen) {
            setOverlayActive(false);
            scheduleFadeOut();
        }
        scheduleWindowClipUpdate();
    };

    // Capture frame hover
    if (captureFrame) {
        captureFrame.addEventListener('mouseenter', onEnterInteractiveArea);
        captureFrame.addEventListener('mouseleave', onLeaveInteractiveArea);
    }

    // Subtitle container hover
    if (subtitleContainer) {
        subtitleContainer.addEventListener('mouseenter', onEnterInteractiveArea);
        subtitleContainer.addEventListener('mouseleave', onLeaveInteractiveArea);
    }

    // Note: We don't use document-level mousemove tracking because:
    // 1. The overlay body has pointer-events: none, so document won't receive mouse events
    // 2. Only the capture-frame and subtitle-container receive mouse events
    // 3. This ensures clean enter/leave detection without false triggers
}

// =============================================================================
// RESIZE HANDLES
// =============================================================================

function setupResizeHandles(captureFrame) {
    if (!captureFrame) return;

    const handles = captureFrame.querySelectorAll('.resize-handle');

    handles.forEach(handle => {
        handle.addEventListener('mousedown', (e) => {
            e.preventDefault();
            e.stopPropagation();

            if (!overlayState.region) return;

            overlayState.isResizing = true;
            overlayState.resizeHandle = handle.dataset.position;
            overlayState.dragStart = {
                x: e.clientX,
                y: e.clientY,
                regionX: overlayState.region.x,
                regionY: overlayState.region.y,
                regionW: overlayState.region.width,
                regionH: overlayState.region.height,
            };

            cancelClickThrough();
            updateInteractionClasses();
            document.body.style.cursor = getResizeCursor(overlayState.resizeHandle);
        });
    });

    // Document-level mouse move and up for resizing
    document.addEventListener('mousemove', handleResize);
    document.addEventListener('mouseup', handleResizeEnd);
}

function getResizeCursor(position) {
    const cursors = {
        'nw': 'nwse-resize', 'n': 'ns-resize', 'ne': 'nesw-resize',
        'w': 'ew-resize', 'e': 'ew-resize',
        'sw': 'nesw-resize', 's': 'ns-resize', 'se': 'nwse-resize',
    };
    return cursors[position] || 'default';
}

function handleResize(e) {
    if (!overlayState.isResizing || !overlayState.region) return;

    const { x: startX, y: startY, regionX, regionY, regionW, regionH } = overlayState.dragStart;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    const handle = overlayState.resizeHandle;

    let newX = regionX, newY = regionY, newW = regionW, newH = regionH;

    // Calculate new dimensions based on which handle is being dragged
    if (handle.includes('w')) {
        newX = regionX + dx;
        newW = regionW - dx;
    }
    if (handle.includes('e')) {
        newW = regionW + dx;
    }
    if (handle.includes('n')) {
        newY = regionY + dy;
        newH = regionH - dy;
    }
    if (handle.includes('s')) {
        newH = regionH + dy;
    }

    // Enforce minimum size
    const minSize = 50;
    if (newW < minSize) {
        if (handle.includes('w')) newX = regionX + regionW - minSize;
        newW = minSize;
    }
    if (newH < minSize) {
        if (handle.includes('n')) newY = regionY + regionH - minSize;
        newH = minSize;
    }

    // Update local state and UI immediately
    overlayState.region = { x: newX, y: newY, width: newW, height: newH };

    const captureFrame = document.getElementById('capture-frame');
    const subtitleContainer = document.getElementById('subtitle-container');

    updateCaptureFrame(captureFrame, overlayState.region);
    updateSubtitlePosition(subtitleContainer, overlayState.region);
    scheduleWindowClipUpdate();

    // Update debug display
    const debugRegion = document.getElementById('debug-region');
    if (debugRegion) {
        debugRegion.textContent = `Region: (${newX}, ${newY}) ${newW}x${newH}`;
    }
}

async function handleResizeEnd() {
    if (!overlayState.isResizing) return;

    overlayState.isResizing = false;
    overlayState.resizeHandle = null;
    document.body.style.cursor = 'default';
    updateInteractionClasses();

    // Save the new region to backend
    await saveRegionToBackend();

    scheduleFadeOut();
}

// =============================================================================
// DRAG TO REPOSITION
// =============================================================================

function setupDragToReposition(captureFrame) {
    if (!captureFrame) return;

    // The frame itself (not handles) is draggable
    captureFrame.addEventListener('mousedown', (e) => {
        // Ignore if clicking on a resize handle or if already resizing
        if (e.target.classList.contains('resize-handle') || overlayState.isResizing) return;
        if (!overlayState.region) return;

        e.preventDefault();

        overlayState.isDragging = true;
        overlayState.dragStart = {
            x: e.clientX,
            y: e.clientY,
            regionX: overlayState.region.x,
            regionY: overlayState.region.y,
            regionW: overlayState.region.width,
            regionH: overlayState.region.height,
        };

        cancelClickThrough();
        updateInteractionClasses();
        document.body.style.cursor = 'move';
    });

    document.addEventListener('mousemove', handleDrag);
    document.addEventListener('mouseup', handleDragEnd);
}

function handleDrag(e) {
    if (!overlayState.isDragging || !overlayState.region) return;

    const { x: startX, y: startY, regionX, regionY, regionW, regionH } = overlayState.dragStart;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;

    const newX = regionX + dx;
    const newY = regionY + dy;

    // Update local state and UI immediately
    overlayState.region = { x: newX, y: newY, width: regionW, height: regionH };

    const captureFrame = document.getElementById('capture-frame');
    const subtitleContainer = document.getElementById('subtitle-container');

    updateCaptureFrame(captureFrame, overlayState.region);
    updateSubtitlePosition(subtitleContainer, overlayState.region);
    scheduleWindowClipUpdate();

    // Update debug display
    const debugRegion = document.getElementById('debug-region');
    if (debugRegion) {
        debugRegion.textContent = `Region: (${newX}, ${newY}) ${regionW}x${regionH}`;
    }
}

async function handleDragEnd() {
    if (!overlayState.isDragging) return;

    overlayState.isDragging = false;
    document.body.style.cursor = 'default';
    updateInteractionClasses();

    // Save the new region to backend
    await saveRegionToBackend();

    scheduleFadeOut();
}

// =============================================================================
// SAVE REGION TO BACKEND
// =============================================================================

async function saveRegionToBackend() {
    if (!overlayState.region) return;

    try {
        const { x, y, width, height } = overlayState.region;
        const scaleFactor = await refreshScaleFactor();
        await window.__TAURI__.core.invoke('set_capture_region', {
            x, y, width, height, scaleFactor
        });
        console.log('💾 Region saved:', overlayState.region);
    } catch (e) {
        console.error('Failed to save region:', e);
    }
}

// =============================================================================
// EVENT LISTENERS
// =============================================================================

async function setupEventListeners(elements) {
    const {
        captureFrame,
        subtitleContainer,
        subtitleHint,
        subtitleHintText,
        subtitleText,
        debugRegion,
        debugStatus,
    } = elements;

    try {
        // Region updates
        await window.__TAURI__.event.listen('overlay-update-region', (event) => {
            const region = event.payload;
            console.log('📍 Region update:', region);

            // Only update if not currently dragging/resizing
            if (!overlayState.isDragging && !overlayState.isResizing) {
                overlayState.region = region;
                updateCaptureFrame(captureFrame, region);
                updateSubtitlePosition(subtitleContainer, region);
                scheduleWindowClipUpdate();

                if (debugRegion) {
                    debugRegion.textContent = `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
                }
            }
        });

        // Translation updates
        await window.__TAURI__.event.listen('translation-update', (event) => {
            if (!window.PipelineUpdate.shouldAccept(
                overlayState.lastPipelinePosition,
                event.payload
            )) {
                console.debug('Discarded stale overlay update');
                return;
            }
            overlayState.lastPipelinePosition =
                window.PipelineUpdate.position(event.payload) ||
                overlayState.lastPipelinePosition;
            const { translated, timestamp, backendUsed, warnings, displayState } = event.payload;
            const presentation = getTranslationPresentation(displayState, backendUsed);
            console.log('🌐 Translation state:', presentation.state);

            const surface = resolveSubtitleSurface(presentation, translated);
            if (surface.mode === 'text') {
                updateSubtitleText(subtitleText, translated, subtitleContainer);
                updateSubtitleHint(
                    subtitleHint,
                    subtitleHintText,
                    backendUsed,
                    warnings
                );
            } else if (surface.mode === 'clear') {
                overlayState.currentText = '';
                subtitleText.textContent = '';
                setSubtitleContainerVisible(subtitleContainer, false);
                clearSubtitleHint(subtitleHint, subtitleHintText);
            } else {
                // Hint-only states must keep the box on screen; otherwise a warming,
                // unavailable, or source-only pipeline looks identical to a dead one.
                overlayState.currentText = '';
                subtitleText.textContent = '';
                setSubtitleHint(
                    subtitleHint,
                    subtitleHintText,
                    presentation.hint,
                    presentation.severity,
                    presentation.persist
                );
                setSubtitleContainerVisible(subtitleContainer, surface.showContainer);
            }

            // Reposition using the real subtitle container height (hint can change size).
            requestAnimationFrame(() => {
                if (overlayState.region) {
                    updateSubtitlePosition(subtitleContainer, overlayState.region);
                    scheduleWindowClipUpdate();
                }
            });
            if (debugStatus) {
                const time = new Date(timestamp).toLocaleTimeString();
                const backend = backendUsed || 'unknown';
                const warningCount = Array.isArray(warnings) ? warnings.length : 0;
                const backendLabel = backend === 'mock' ? 'mock (source only)' : backend;
                debugStatus.textContent = warningCount > 0
                    ? `State: ${presentation.state} · ${backendLabel} @ ${time} | warnings: ${warningCount}`
                    : `State: ${presentation.state} · ${backendLabel} @ ${time}`;
            }
        });

        // Visibility changes
        await window.__TAURI__.event.listen('overlay-visibility', async (event) => {
            const visible = event.payload;
            console.log('👁️ Visibility:', visible);
            overlayState.isVisible = visible;

            if (visible) {
                await refreshScaleFactor();
                // Ensure the capture frame is visible immediately (it may still be faded from a previous session)
                showCaptureFrame();
                captureFrame.classList.remove('exiting');
                subtitleContainer.classList.remove('exiting');
                // Fetch region and show
                try {
                    const region = await window.__TAURI__.core.invoke('get_capture_region');
                    if (region) {
                        overlayState.region = region;
                        updateCaptureFrame(captureFrame, region);
                        updateSubtitlePosition(subtitleContainer, region);
                        scheduleWindowClipUpdate();
                        if (debugRegion) {
                            debugRegion.textContent = `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
                        }
                    }
                } catch (e) {
                    console.error('Failed to fetch region:', e);
                }

                captureFrame.classList.remove('hidden');
                captureFrame.classList.add('visible');
                captureFrame.classList.remove('faded');
                captureFrame.classList.add('entering');
                requestAnimationFrame(() => captureFrame.classList.remove('entering'));
                scheduleWindowClipUpdate();

                // Start fade timer (frame fades after inactivity)
                scheduleFadeOut();
                startClickThroughMonitor();
                await setOverlayClickThrough(true);
            } else {
                // Fade everything out (Rust will hide the window after a short delay).
                // Keep `visible` during the fade so the Win32 window region stays clipped.
                captureFrame.classList.remove('entering');
                captureFrame.classList.remove('faded');
                captureFrame.classList.remove('hidden');
                captureFrame.classList.add('visible');
                captureFrame.classList.add('exiting');

                if (subtitleContainer.classList.contains('visible')) {
                    subtitleContainer.classList.add('exiting');
                }

                scheduleWindowClipUpdate();

                // Hide hint (if any)
                clearSubtitleHint(subtitleHint, subtitleHintText);

                // Reset cached subtitle text so restarting translation shows the first line even
                // if it's identical to the previous session.
                overlayState.currentText = '';
                if (subtitleText) {
                    subtitleText.textContent = '';
                }

                // Clear fade timer
                if (fadeTimer) clearTimeout(fadeTimer);
                stopClickThroughMonitor();
                await setOverlayClickThrough(true);

                // Final cleanup after the fade (best effort).
                setTimeout(() => {
                    captureFrame.classList.add('hidden');
                    captureFrame.classList.remove('visible', 'exiting', 'faded');

                    subtitleContainer.classList.remove('exiting');
                    setSubtitleContainerVisible(subtitleContainer, false);
                }, OVERLAY_VISIBILITY_FADE_MS);
            }
        });

        // Listen for settings updates from main UI
        await window.__TAURI__.event.listen('overlay-settings-updated', async (event) => {
            const payload = event.payload || {};
            console.log('⚙️ Overlay settings updated from main UI:', payload);

            // Update all overlay appearance settings
            if (typeof payload.fontSize === 'number') {
                overlayState.fontSize = payload.fontSize;
                const fontSizeSlider = document.getElementById('font-size-slider');
                const fontSizeDisplay = document.getElementById('font-size-display');
                if (fontSizeSlider) fontSizeSlider.value = payload.fontSize;
                if (fontSizeDisplay) fontSizeDisplay.textContent = `${payload.fontSize}px`;
            }

            if (typeof payload.fontFamily === 'string') {
                overlayState.fontFamily = payload.fontFamily;
            }

            if (typeof payload.textColor === 'string') {
                overlayState.textColor = payload.textColor;
            }

            if (typeof payload.backgroundColor === 'string') {
                overlayState.backgroundColor = payload.backgroundColor;
            }

            // Apply all styles to subtitle elements
            applyOverlayStyles();

            // Reposition subtitle if needed (font size affects layout)
            if (overlayState.region && subtitleContainer) {
                updateSubtitlePosition(subtitleContainer, overlayState.region);
            }

            // Update diagnostics visibility
            if (typeof payload.showDiagnostics === 'boolean') {
                overlayState.showDiagnostics = payload.showDiagnostics;
                updateDiagnosticsVisibility();
                const diagnosticsToggle = document.getElementById('diagnostics-toggle');
                if (diagnosticsToggle) diagnosticsToggle.checked = payload.showDiagnostics;
            }
        });

        console.log('✅ Event listeners set up');
    } catch (error) {
        console.error('❌ Failed to set up event listeners:', error);
    }
}

// =============================================================================
// UPDATE FUNCTIONS
// =============================================================================

function updateCaptureFrame(frame, region) {
    if (!region || !frame) return;

    frame.style.left = `${region.x}px`;
    frame.style.top = `${region.y}px`;
    frame.style.width = `${region.width}px`;
    frame.style.height = `${region.height}px`;
}

function updateSubtitlePosition(container, region) {
    if (!region || !container) return;

    const padding = 10;
    const screenHeight = window.innerHeight;
    // Prefer measured height when visible (includes hint row), fallback to a conservative estimate.
    const measured = container.classList.contains('visible')
        ? Math.round(container.getBoundingClientRect().height)
        : 0;
    const estimatedSubtitleHeight = measured > 0
        ? measured
        : Math.round(overlayState.fontSize * 1.4 + 54);

    const spaceBelow = screenHeight - (region.y + region.height + padding);
    const spaceAbove = region.y - padding;

    let top;
    if (spaceBelow >= estimatedSubtitleHeight) {
        top = region.y + region.height + padding;
    } else if (spaceAbove >= estimatedSubtitleHeight) {
        top = region.y - padding - estimatedSubtitleHeight;
    } else {
        top = region.y + region.height + padding;
    }

    container.style.width = `${region.width}px`;
    container.style.maxWidth = `${region.width}px`;
    container.style.left = `${region.x}px`;
    container.style.top = `${top}px`;
    container.style.transform = 'none';
}

// Single owner of the subtitle box visibility classes. The Win32 window region
// only includes the box while it is `visible`, so class state and clip state
// must always be updated together.
function setSubtitleContainerVisible(container, visible) {
    if (!container) return;
    container.classList.toggle('visible', visible);
    container.classList.toggle('hidden', !visible);
    scheduleWindowClipUpdate();
}

function updateSubtitleText(textElement, newText, container) {
    if (!newText || newText.trim() === '') {
        setSubtitleContainerVisible(container, false);
        return;
    }

    if (newText === overlayState.currentText) return;

    overlayState.currentText = newText;
    textElement.classList.remove('fade-in');
    void textElement.offsetWidth;
    textElement.textContent = newText;
    textElement.classList.add('fade-in');

    setSubtitleContainerVisible(container, true);
}

// =============================================================================
// WINDOW REGION CLIPPING (Windows transparency workaround)
// =============================================================================

// Restrict the overlay to visible UI so WebView2 opacity regressions cannot cover the screen.
let clipUpdateLoopRunning = false;
let clipUpdateLoopUntilMs = 0;

function scheduleWindowClipUpdate() {
    // Many overlay elements (capture frame + subtitle container) animate position/size
    // with CSS transitions. If we only set the window region once, those transitions can
    // get clipped mid-animation. To keep things looking smooth, update the window region
    // for a short period after any change.
    clipUpdateLoopUntilMs = Date.now() + 350;
    if (clipUpdateLoopRunning) return;

    clipUpdateLoopRunning = true;
    requestAnimationFrame(runWindowClipUpdateLoop);
}

async function updateOverlayWindowClip() {
    if (!window.__TAURI__?.core?.invoke) return;

    const captureFrame = document.getElementById('capture-frame');
    const subtitleContainer = document.getElementById('subtitle-container');
    const debugInfo = document.getElementById('debug-info');
    const settingsButton = document.getElementById('settings-button');
    const settingsMenu = document.getElementById('settings-menu');

    const frameVisible = captureFrame &&
        captureFrame.classList.contains('visible') &&
        !captureFrame.classList.contains('faded');

    const frameRegion = frameVisible && overlayState.region ? overlayState.region : null;

    let subtitleBounds = null;
    if (subtitleContainer && subtitleContainer.classList.contains('visible') && !subtitleContainer.classList.contains('hidden')) {
        const rect = subtitleContainer.getBoundingClientRect();
        subtitleBounds = {
            x: Math.round(rect.left),
            y: Math.round(rect.top),
            width: Math.round(rect.width),
            height: Math.round(rect.height),
        };
    }

    try {
        // IMPORTANT: DOM coordinates are in CSS pixels (logical units).
        // Win32 window regions (SetWindowRgn) use device pixels, so we pass the window
        // scale factor and let Rust convert everything to physical pixels.
        const scaleFactor = await getScaleFactor();

        // Keep the frame ring tight. Controls positioned outside the frame must
        // be passed as their own small rectangles or they get clipped.
        const bounds = [];
        const radii = [];

        if (captureFrame && frameVisible) {
            captureFrame.querySelectorAll('.resize-handle')
                .forEach((handle) => appendClipSurface(bounds, radii, handle));
        }

        if (settingsButton && frameVisible) {
            appendClipSurface(bounds, radii, settingsButton);
        }

        // Keep the bottom-right diagnostics panel visible when window clipping is enabled.
        if (debugInfo) {
            const style = getComputedStyle(debugInfo);
            const opacity = parseFloat(style.opacity || '0');
            if (style.display !== 'none' && opacity > 0.05) {
                appendClipSurface(bounds, radii, debugInfo);
            }
        }

        // Include visible controls that are not children of the frame.
        if (settingsMenu && overlayState.settingsOpen) {
            appendClipSurface(bounds, radii, settingsMenu);
        }

        const handleBounds = bounds.length > 0 ? bounds : null;
        const controlRadii = radii.length > 0 ? radii : null;

        await window.__TAURI__.core.invoke('set_overlay_window_clip', {
            frameRegion, subtitleBounds, handleBounds, controlRadii, scaleFactor,
        });
    } catch (e) {
        // Ignore - this is a best-effort platform workaround.
    }
}

async function runWindowClipUpdateLoop() {
    try {
        await updateOverlayWindowClip();
    } finally {
        if (Date.now() < clipUpdateLoopUntilMs) {
            requestAnimationFrame(runWindowClipUpdateLoop);
        } else {
            clipUpdateLoopRunning = false;
        }
    }
}

// =============================================================================
// SETTINGS FUNCTIONS
// =============================================================================

async function loadOverlaySettings() {
    try {
        const settings = await window.__TAURI__.core.invoke('get_settings');
        if (settings?.overlay) {
            // Load all overlay settings into state
            overlayState.fontSize = settings.overlay.fontSize || 24;
            overlayState.fontFamily = settings.overlay.fontFamily || 'Segoe UI';
            overlayState.textColor = settings.overlay.textColor || '#FFFFFF';
            overlayState.backgroundColor = settings.overlay.backgroundColor || 'rgba(0, 0, 0, 0.75)';
            overlayState.showDiagnostics = settings.overlay.showDiagnostics === true;

            // Apply all styles to subtitle elements
            applyOverlayStyles();

            // Apply diagnostics visibility
            updateDiagnosticsVisibility();

            // Sync the toggle checkbox if it exists
            const diagnosticsToggle = document.getElementById('diagnostics-toggle');
            if (diagnosticsToggle) {
                diagnosticsToggle.checked = overlayState.showDiagnostics;
            }

            console.log('🎨 Loaded overlay settings:', {
                fontSize: overlayState.fontSize,
                fontFamily: overlayState.fontFamily,
                textColor: overlayState.textColor,
                backgroundColor: overlayState.backgroundColor,
                showDiagnostics: overlayState.showDiagnostics,
            });
        }
    } catch (e) {
        console.error('Failed to load settings:', e);
    }
}

function applyOverlayStyles() {
    const subtitleText = document.getElementById('subtitle-text');
    const subtitleContainer = document.getElementById('subtitle-container');

    if (subtitleText) {
        subtitleText.style.fontSize = `${overlayState.fontSize}px`;
        subtitleText.style.fontFamily = overlayState.fontFamily;
        subtitleText.style.color = overlayState.textColor;
    }

    if (subtitleContainer) {
        // Use 'background' to override the CSS shorthand property
        subtitleContainer.style.background = overlayState.backgroundColor;
    }

    console.log('🎨 Applied overlay styles:', {
        fontSize: overlayState.fontSize,
        fontFamily: overlayState.fontFamily,
        textColor: overlayState.textColor,
        backgroundColor: overlayState.backgroundColor,
    });
}

function updateDiagnosticsVisibility() {
    const debugInfo = document.getElementById('debug-info');
    if (!debugInfo) return;

    if (overlayState.showDiagnostics) {
        debugInfo.classList.remove('hidden');
        debugInfo.classList.add('visible');
    } else {
        debugInfo.classList.add('hidden');
        debugInfo.classList.remove('visible');
    }
}

async function saveOverlaySettings() {
    try {
        const settings = await window.__TAURI__.core.invoke('get_settings');
        settings.overlay.fontSize = overlayState.fontSize;
        settings.overlay.showDiagnostics = overlayState.showDiagnostics;
        await window.__TAURI__.core.invoke('save_settings', { settings });
        console.log('💾 Saved overlay settings:', { fontSize: overlayState.fontSize, showDiagnostics: overlayState.showDiagnostics });
    } catch (e) {
        console.error('Failed to save settings:', e);
    }
}

function setupSettingsButton(button, menu, subtitleText, subtitleContainer) {
    setupSettingsMenu({
        button,
        menu,
        closeButton: document.getElementById('settings-close'),
        fontSizeSlider: document.getElementById('font-size-slider'),
        fontSizeDisplay: document.getElementById('font-size-display'),
        diagnosticsToggle: document.getElementById('diagnostics-toggle'),
        initialFontSize: overlayState.fontSize,
        initialDiagnostics: overlayState.showDiagnostics,
        onOpenChange: (open) => {
            overlayState.settingsOpen = open;
            if (open) {
                showCaptureFrame();
            } else {
                scheduleFadeOut();
            }
            scheduleWindowClipUpdate();
        },
        onFontSize: (newSize) => {
            overlayState.fontSize = newSize;
            if (subtitleText) subtitleText.style.fontSize = `${newSize}px`;
            if (overlayState.region && subtitleContainer) {
                updateSubtitlePosition(subtitleContainer, overlayState.region);
            }
        },
        onDiagnostics: (enabled) => {
            overlayState.showDiagnostics = enabled;
            updateDiagnosticsVisibility();
        },
        onCommit: () => saveOverlaySettings(),
    });
}

// =============================================================================
// EXPORTS
// =============================================================================

window.OverlayController = {
    state: overlayState,
};
