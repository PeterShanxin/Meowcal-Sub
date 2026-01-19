// =============================================================================
// OVERLAY.JS - Overlay Window Controller
// =============================================================================
// This script controls the overlay window which displays:
// 1. A border around the capture region (with resize handles)
// 2. Translated subtitles below/above the capture region
//
// Features:
// - Smart click-through management (clickable when hovered, click-through otherwise)
// - Capture frame auto-fades after inactivity, reappears on hover
// - 8 resize handles for live region resizing
// - Drag to reposition when hovering inside the frame
// =============================================================================

// Wait for Tauri to be ready
document.addEventListener('DOMContentLoaded', () => {
    console.log('🎬 Overlay loaded!');
    initOverlay();
});

// =============================================================================
// GLOBAL STATE
// =============================================================================

const overlayState = {
    region: null,           // { x, y, width, height }
    isVisible: false,       // Whether translation is active
    isOverlayActive: false, // Whether user is interacting with overlay
    currentText: '',
    debugMode: true,
    fontSize: 24,
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
        const handlePadding = 12;
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
    clickThroughMonitor = setInterval(updateClickThroughState, 80);
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
        if (!overlayState.isOverlayActive && !overlayState.isDragging && !overlayState.isResizing) {
            captureFrame.classList.add('faded');
        }
    }, 4000);
}

function showCaptureFrame() {
    if (fadeTimer) clearTimeout(fadeTimer);

    const captureFrame = document.getElementById('capture-frame');
    if (captureFrame) {
        captureFrame.classList.remove('faded');
    }
}

// =============================================================================
// INITIALIZATION
// =============================================================================

async function initOverlay() {
    console.log('🔧 Initializing overlay...');

    const captureFrame = document.getElementById('capture-frame');
    const subtitleContainer = document.getElementById('subtitle-container');
    const subtitleText = document.getElementById('subtitle-text');
    const debugRegion = document.getElementById('debug-region');
    const debugStatus = document.getElementById('debug-status');
    const settingsButton = document.getElementById('settings-button');
    const settingsMenu = document.getElementById('settings-menu');

    if (debugStatus) debugStatus.textContent = 'Status: setting up...';

    await refreshScaleFactor();

    // Load initial font size
    await loadOverlaySettings(subtitleText);

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
    };

    const onLeaveInteractiveArea = () => {
        if (!overlayState.isDragging && !overlayState.isResizing && !overlayState.settingsOpen) {
            setOverlayActive(false);
            scheduleFadeOut();
        }
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
    const { captureFrame, subtitleContainer, subtitleText, debugRegion, debugStatus } = elements;

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

                if (debugRegion) {
                    debugRegion.textContent = `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
                }
            }
        });

        // Translation updates
        await window.__TAURI__.event.listen('translation-update', (event) => {
            const { translated, timestamp } = event.payload;
            console.log('🌐 Translation:', translated);
            updateSubtitleText(subtitleText, translated, subtitleContainer);
            if (debugStatus) {
                debugStatus.textContent = `Last: ${new Date(timestamp).toLocaleTimeString()}`;
            }
        });

        // Visibility changes
        await window.__TAURI__.event.listen('overlay-visibility', async (event) => {
            const visible = event.payload;
            console.log('👁️ Visibility:', visible);
            overlayState.isVisible = visible;

            if (visible) {
                await refreshScaleFactor();
                // Fetch region and show
                try {
                    const region = await window.__TAURI__.core.invoke('get_capture_region');
                    if (region) {
                        overlayState.region = region;
                        updateCaptureFrame(captureFrame, region);
                        updateSubtitlePosition(subtitleContainer, region);
                        if (debugRegion) {
                            debugRegion.textContent = `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
                        }
                    }
                } catch (e) {
                    console.error('Failed to fetch region:', e);
                }

                captureFrame.classList.remove('hidden');
                captureFrame.classList.add('visible');

                // Start fade timer (frame fades after inactivity)
                scheduleFadeOut();
                startClickThroughMonitor();
                await setOverlayClickThrough(true);
            } else {
                // Hide everything
                captureFrame.classList.add('hidden');
                captureFrame.classList.remove('visible');
                subtitleContainer.classList.add('hidden');
                subtitleContainer.classList.remove('visible');

                // Clear fade timer
                if (fadeTimer) clearTimeout(fadeTimer);
                stopClickThroughMonitor();
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

    frame.classList.remove('hidden');
    frame.classList.add('visible');
}

function updateSubtitlePosition(container, region) {
    if (!region || !container) return;

    const padding = 10;
    const screenHeight = window.innerHeight;
    const estimatedSubtitleHeight = overlayState.fontSize * 1.4 + 24;

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

function updateSubtitleText(textElement, newText, container) {
    if (!newText || newText.trim() === '') {
        container.classList.add('hidden');
        container.classList.remove('visible');
        return;
    }

    if (newText === overlayState.currentText) return;

    overlayState.currentText = newText;
    textElement.classList.remove('fade-in');
    void textElement.offsetWidth;
    textElement.textContent = newText;
    textElement.classList.add('fade-in');

    container.classList.remove('hidden');
    container.classList.add('visible');
}

// =============================================================================
// SETTINGS FUNCTIONS
// =============================================================================

async function loadOverlaySettings(subtitleText) {
    try {
        const settings = await window.__TAURI__.core.invoke('get_settings');
        if (settings?.overlay) {
            overlayState.fontSize = settings.overlay.fontSize || 24;
            if (subtitleText) {
                subtitleText.style.fontSize = `${overlayState.fontSize}px`;
            }
        }
    } catch (e) {
        console.error('Failed to load settings:', e);
    }
}

async function saveOverlaySettings() {
    try {
        const settings = await window.__TAURI__.core.invoke('get_settings');
        settings.overlay.fontSize = overlayState.fontSize;
        await window.__TAURI__.core.invoke('save_settings', { settings });
        console.log('💾 Saved font size:', overlayState.fontSize);
    } catch (e) {
        console.error('Failed to save settings:', e);
    }
}

function setupSettingsButton(button, menu, subtitleText, subtitleContainer) {
    if (!button || !menu) return;

    const fontSizeSlider = document.getElementById('font-size-slider');
    const fontSizeDisplay = document.getElementById('font-size-display');

    if (fontSizeSlider) fontSizeSlider.value = overlayState.fontSize;
    if (fontSizeDisplay) fontSizeDisplay.textContent = `${overlayState.fontSize}px`;

    // Toggle menu
    button.addEventListener('click', (e) => {
        e.stopPropagation();
        overlayState.settingsOpen = !overlayState.settingsOpen;
        menu.classList.toggle('visible', overlayState.settingsOpen);
        menu.classList.toggle('hidden', !overlayState.settingsOpen);

        if (!overlayState.settingsOpen) {
            scheduleFadeOut();
        }
    });

    // Close on outside click
    document.addEventListener('click', (e) => {
        if (overlayState.settingsOpen && !menu.contains(e.target) && e.target !== button) {
            overlayState.settingsOpen = false;
            menu.classList.remove('visible');
            menu.classList.add('hidden');
            scheduleFadeOut();
        }
    });

    // Font size slider
    if (fontSizeSlider) {
        fontSizeSlider.addEventListener('input', (e) => {
            const newSize = parseInt(e.target.value, 10);
            overlayState.fontSize = newSize;
            if (fontSizeDisplay) fontSizeDisplay.textContent = `${newSize}px`;
            if (subtitleText) subtitleText.style.fontSize = `${newSize}px`;
            if (overlayState.region && subtitleContainer) {
                updateSubtitlePosition(subtitleContainer, overlayState.region);
            }
        });

        fontSizeSlider.addEventListener('change', () => saveOverlaySettings());
    }

    button.style.pointerEvents = 'auto';
    menu.style.pointerEvents = 'auto';

    console.log('⚙️ Settings button initialized');
}

// =============================================================================
// EXPORTS
// =============================================================================

window.OverlayController = {
    state: overlayState,
};
