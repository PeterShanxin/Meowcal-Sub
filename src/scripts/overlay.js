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
const { clipPayloadEquals } = window.OverlayClipPayload;
const { pointInBounds, rectToPhysicalBounds, regionToPhysicalBounds } = window.OverlayHitBounds;
const { buildDiagnosticsText } = window.OverlayDiagnostics;
const { frameScaleTokens, resolveSubtitlePlacement, roundedRectBounds } = window.OverlayGeometry;
const { moveRegion, resizeRegion } = window.RegionGeometry;
const { DEFAULT_APPEARANCE, hydrateAppearance, patchAppearance } = window.OverlayAppearance;
const { createTimerOwner } = window.OverlayTimers;

// Smallest capture region a resize drag may leave behind. Larger than the
// selector's minimum because the overlay frame also has to hold its handles.
const MIN_REGION_SIZE = 50;

// Gap between the capture region and the subtitle plate.
const SUBTITLE_GAP_PX = 10;

// Timer slots and cadences. See overlay-timers.js for why these are owned.
const CLICK_THROUGH_TIMER = 'clickThrough';
const FRAME_FADE_TIMER = 'frameFade';
const HIDE_CLEANUP_TIMER = 'hideCleanup';
const CLICK_THROUGH_POLL_MS = 150;
const CLICK_THROUGH_STALL_MS = 2000;
const FRAME_FADE_DELAY_MS = 4000;

// =============================================================================
// GLOBAL STATE
// =============================================================================

const overlayState = {
    region: null,           // { x, y, width, height }
    isVisible: false,       // Whether translation is active
    isOverlayActive: false, // Whether user is interacting with overlay
    currentText: '',
    debugMode: true,
    // Overlay appearance settings, owned by overlay-appearance.js
    ...DEFAULT_APPEARANCE,
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

// Timers whose lifetime is overlay state, not page lifetime.
const overlayTimers = createTimerOwner();
let clickThroughBusy = false;
let clickThroughBusyUntil = 0;

// Fade duration for "start/stop translation" show/hide transitions.
// Keep this short to avoid delaying stop/start responsiveness.
// IMPORTANT: Must match OVERLAY_HIDE_FADE_MS in src-tauri/src/commands.rs
const OVERLAY_VISIBILITY_FADE_MS = 220;

function syncFrameScaleTokens(scaleFactor) {
    const { borderCss, radiusCss } = frameScaleTokens(scaleFactor);

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

function getWindowOrigin() {
    return { x: window.screenX || 0, y: window.screenY || 0 };
}

function getInteractiveBoundsPhysical(scaleFactor) {
    const bounds = [];
    const origin = getWindowOrigin();

    if (overlayState.region) {
        // Padding covers the resize handles and the settings gear outside the frame.
        bounds.push(regionToPhysicalBounds(overlayState.region, 40, origin, scaleFactor));
    }

    const subtitleContainer = document.getElementById('subtitle-container');
    if (subtitleContainer && !subtitleContainer.classList.contains('hidden')) {
        bounds.push(rectToPhysicalBounds(subtitleContainer.getBoundingClientRect(), origin, scaleFactor));
    }

    const settingsMenu = document.getElementById('settings-menu');
    if (settingsMenu && settingsMenu.classList.contains('visible')) {
        bounds.push(rectToPhysicalBounds(settingsMenu.getBoundingClientRect(), origin, scaleFactor));
    }

    return bounds;
}

async function updateClickThroughState() {
    // The busy flag must never outlive one tick: every step below is a main-thread
    // IPC round trip, and a wedged flag would freeze the overlay in whatever
    // click-through state it happened to be in, with no way back to interactive.
    if (clickThroughBusy && Date.now() < clickThroughBusyUntil) return;
    if (!overlayState.isVisible) return;
    if (!window.__TAURI__?.window?.cursorPosition) return;

    clickThroughBusy = true;
    clickThroughBusyUntil = Date.now() + CLICK_THROUGH_STALL_MS;
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
    if (overlayTimers.isPending(CLICK_THROUGH_TIMER)) return;
    overlayTimers.interval(CLICK_THROUGH_TIMER, updateClickThroughState, CLICK_THROUGH_POLL_MS);
}

function stopClickThroughMonitor() {
    overlayTimers.cancel(CLICK_THROUGH_TIMER);
}

// =============================================================================
// CAPTURE FRAME FADE MANAGEMENT
// =============================================================================

function scheduleFadeOut() {
    overlayTimers.cancel(FRAME_FADE_TIMER);

    const captureFrame = document.getElementById('capture-frame');
    if (!captureFrame) return;

    // Fade out after a few seconds of no interaction
    overlayTimers.timeout(FRAME_FADE_TIMER, () => {
        // The settings popup is anchored to the gear inside the frame, so fading
        // the frame while it is open would strand the popup with no way to close it.
        if (overlayState.settingsOpen) return;
        if (!overlayState.isOverlayActive && !overlayState.isDragging && !overlayState.isResizing) {
            captureFrame.classList.add('faded');
            scheduleWindowClipUpdate();
        }
    }, FRAME_FADE_DELAY_MS);
}

function showCaptureFrame() {
    overlayTimers.cancel(FRAME_FADE_TIMER);

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

    overlayState.region = resizeRegion(
        { x: regionX, y: regionY, width: regionW, height: regionH },
        overlayState.resizeHandle,
        e.clientX - startX,
        e.clientY - startY,
        MIN_REGION_SIZE,
    );

    applyRegionToOverlay(overlayState.region);
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

    overlayState.region = moveRegion(
        { x: regionX, y: regionY, width: regionW, height: regionH },
        e.clientX - startX,
        e.clientY - startY,
    );

    applyRegionToOverlay(overlayState.region);
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
                applyRegionToOverlay(region);
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
            const { original, translated, timestamp, backendUsed, warnings, displayState } = event.payload;
            const presentation = getTranslationPresentation(displayState, backendUsed, warnings);
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
                // Hint-bearing states keep the box on screen, or a warming/unavailable
                // pipeline looks identical to a dead one. 'keep' also holds the line
                // already showing: blanking it for an "engine is behind" banner would
                // cost the viewer the very thing they were reading.
                if (surface.mode !== 'keep') {
                    overlayState.currentText = '';
                    subtitleText.textContent = '';
                }
                setSubtitleHint(subtitleHint, subtitleHintText, presentation.hint,
                    presentation.severity, presentation.persist);
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
                debugStatus.textContent = buildDiagnosticsText({
                    state: presentation.state, backendUsed, warnings,
                    modelMs: event.payload.modelMs, totalMs: event.payload.totalMs,
                    source: original, now: new Date(timestamp).toLocaleTimeString(),
                });
            }
        });

        // Visibility changes
        await window.__TAURI__.event.listen('overlay-visibility', async (event) => {
            const visible = event.payload;
            console.log('👁️ Visibility:', visible);
            overlayState.isVisible = visible;

            if (visible) {
                // A stop inside the fade window leaves its hide cleanup pending.
                // Firing it now would hide the capture frame we are about to
                // show, for the rest of the session.
                overlayTimers.cancel(HIDE_CLEANUP_TIMER);
                // The window is re-shown here, so the cached click-through and clip
                // state can no longer be trusted to match what the OS window has.
                overlayState.isClickThrough = null;
                lastClipPayload = null;
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
                        applyRegionToOverlay(region);
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

                overlayTimers.cancel(FRAME_FADE_TIMER);
                stopClickThroughMonitor();
                await setOverlayClickThrough(true);

                // Final cleanup after the fade (best effort).
                overlayTimers.timeout(HIDE_CLEANUP_TIMER, () => {
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

            const { applied, next } = patchAppearance(overlayState, payload);
            Object.assign(overlayState, next);

            if (applied.includes('fontSize')) {
                const fontSizeSlider = document.getElementById('font-size-slider');
                const fontSizeDisplay = document.getElementById('font-size-display');
                if (fontSizeSlider) fontSizeSlider.value = overlayState.fontSize;
                if (fontSizeDisplay) fontSizeDisplay.textContent = `${overlayState.fontSize}px`;
            }

            // Apply all styles to subtitle elements
            applyOverlayStyles();

            // Reposition subtitle if needed (font size affects layout)
            if (overlayState.region && subtitleContainer) {
                updateSubtitlePosition(subtitleContainer, overlayState.region);
            }

            // Update diagnostics visibility
            if (applied.includes('showDiagnostics')) {
                updateDiagnosticsVisibility();
                const diagnosticsToggle = document.getElementById('diagnostics-toggle');
                if (diagnosticsToggle) diagnosticsToggle.checked = overlayState.showDiagnostics;
            }
        });

        // Liveness: ready only after every required listener above registered (#112).
        window.OverlayLiveness.signalReady();
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

    // Prefer measured height when visible (includes hint row), fallback to a conservative estimate.
    const measured = container.classList.contains('visible')
        ? Math.round(container.getBoundingClientRect().height)
        : 0;

    const placement = resolveSubtitlePlacement(
        region,
        window.innerHeight,
        measured,
        overlayState.fontSize,
        SUBTITLE_GAP_PX,
    );

    container.style.width = `${placement.width}px`;
    container.style.maxWidth = `${placement.width}px`;
    container.style.left = `${placement.left}px`;
    container.style.top = `${placement.top}px`;
    container.style.transform = 'none';
}

// Single owner of "the capture region moved". The frame, the subtitle plate,
// the Win32 clip, and the diagnostics readout are four views of one rectangle;
// updating any of them alone leaves the overlay drawn in one place and clipped
// in another. `initOverlay` deliberately does not call this - it paints before
// the window is shown, and a clip issued against a hidden window is exactly the
// stale-clip state #113 exists to detect.
function applyRegionToOverlay(region) {
    if (!region) return;

    updateCaptureFrame(document.getElementById('capture-frame'), region);
    updateSubtitlePosition(document.getElementById('subtitle-container'), region);
    scheduleWindowClipUpdate();

    const debugRegion = document.getElementById('debug-region');
    if (debugRegion) {
        debugRegion.textContent =
            `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
    }
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
let lastClipPayload = null;

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
        subtitleBounds = roundedRectBounds(subtitleContainer.getBoundingClientRect());
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

        const payload = {
            frameRegion,
            subtitleBounds,
            handleBounds: bounds.length > 0 ? bounds : null,
            controlRadii: radii.length > 0 ? radii : null,
            scaleFactor,
        };

        // The settle loop runs every animation frame while geometry animates, but
        // the command is synchronous on the Tauri main thread. Resending identical
        // geometry starves capture, OCR, and the click-through monitor.
        if (clipPayloadEquals(payload, lastClipPayload)) return;

        await window.__TAURI__.core.invoke('set_overlay_window_clip', payload);
        lastClipPayload = payload;
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
            Object.assign(overlayState, hydrateAppearance(settings.overlay));

            // Apply all styles to subtitle elements
            applyOverlayStyles();

            // Apply diagnostics visibility
            updateDiagnosticsVisibility();

            // Sync the toggle checkbox if it exists
            const diag = document.getElementById('diagnostics-toggle');
            if (diag) diag.checked = overlayState.showDiagnostics;
            const light = document.getElementById('light-background-toggle');
            if (light) light.checked = overlayState.lightBackground;

            console.log('🎨 Loaded overlay settings:', settings.overlay);
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
        subtitleText.style.color = overlayState.lightBackground ? '' : overlayState.textColor;
    }

    if (subtitleContainer) {
        // Never write an inline background here. The plate colour is opaque in
        // CSS on purpose - the layered window supplies the translucency - and an
        // rgba() written inline composited against WebView2's opaque white
        // backing, which is what made the plate look washed-out white.
        subtitleContainer.classList.toggle('light', overlayState.lightBackground === true);
    }

    console.log('🎨 Applied overlay styles:', overlayState.fontSize, overlayState.fontFamily);
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

    // The window region still contains the old panel rectangle until it is
    // rebuilt, and a clipped area with nothing painted in it renders as a solid
    // white block over the video.
    scheduleWindowClipUpdate();
}

async function saveOverlaySettings() {
    try {
        const settings = await window.__TAURI__.core.invoke('get_settings');
        Object.assign(settings.overlay, { fontSize: overlayState.fontSize, showDiagnostics: overlayState.showDiagnostics, lightBackground: overlayState.lightBackground });
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
        lightToggle: document.getElementById('light-background-toggle'),
        initialLight: overlayState.lightBackground,
        onLight: (on) => { overlayState.lightBackground = on; applyOverlayStyles(); },
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
