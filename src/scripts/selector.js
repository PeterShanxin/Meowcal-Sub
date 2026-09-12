// =============================================================================
// SELECTOR.JS - Area Selection Logic
// =============================================================================
// Handles choosing the subtitle capture region with the mouse or the keyboard.
//
// Flow:
// 1. Drag across one line of subtitles, or press Enter to place a box
// 2. Drag, resize, or use the arrow keys to adjust it
// 3. Enter or "Use this area" saves it; Redraw starts again
// 4. Esc, right-click, or Cancel closes without saving
// =============================================================================

const {
    actionBarTop,
    arrowDelta,
    buildCaptureRegionPayload,
    buildDimOverlaySegments,
    defaultSelectionRect,
    meetsMinimumSelection,
    screenRectToClientRect,
    selectionRectFromPoints,
} = window.SelectorGeometry;
// Shared with the overlay, which enforces a larger minimum rectangle.
const { moveRegion, resizeRegion } = window.RegionGeometry;
const MIN_RESIZE_SIZE = 30;
const ACTION_BAR_GAP = 12;

// Shown in the hint bar instead of a native dialog or a silently dropped drag (#76).
const TOO_SMALL_MESSAGE = 'Area too small. Drag across one full line of subtitles.';
const SAVE_FAILED_MESSAGE = 'Couldn’t save the area. Try again.';

// Selection state
const state = {
    isSelecting: false,
    hasSelection: false,
    isSaving: false,
    startX: 0,
    startY: 0,
    currentX: 0,
    currentY: 0,
    region: null,
};

// DOM elements - will be set after DOM loads
let selectionBox = null;
let actionButtons = null;
let confirmBtn = null;
let retryBtn = null;
let cancelBtn = null;
let errorText = null;
let desktopSnapshot = null;
let overlayTop = null;
let overlayLeft = null;
let overlayRight = null;
let overlayBottom = null;

// =============================================================================
// INITIALIZATION
// =============================================================================

document.addEventListener('DOMContentLoaded', async () => {
    // Force transparent background via WebView2 API (workaround for Tauri 2.0 transparency issues)
    // On Windows 8+, alpha=0 creates true transparency
    try {
        const currentWebview = window.__TAURI__.webview.getCurrentWebview();
        await currentWebview.setBackgroundColor([0, 0, 0, 0]);
    } catch (e) {
        console.warn('Could not set transparent background via webview API:', e);
        // Fallback: try window API (WebviewWindow combines window + webview)
        try {
            const currentWindow = window.__TAURI__.window.getCurrentWindow();
            await currentWindow.setBackgroundColor([0, 0, 0, 0]);
        } catch (e2) {
            console.warn('Could not set transparent background:', e2);
        }
    }

    // Get DOM elements
    selectionBox = document.getElementById('selection-box');
    actionButtons = document.getElementById('action-buttons');
    confirmBtn = document.getElementById('confirm-btn');
    retryBtn = document.getElementById('retry-btn');
    cancelBtn = document.getElementById('cancel-btn');
    errorText = document.getElementById('selector-error');
    desktopSnapshot = document.getElementById('desktop-snapshot');
    overlayTop = document.getElementById('overlay-top');
    overlayLeft = document.getElementById('overlay-left');
    overlayRight = document.getElementById('overlay-right');
    overlayBottom = document.getElementById('overlay-bottom');

    if (!selectionBox || !actionButtons) {
        console.error('Failed to find required DOM elements');
        return;
    }

    // Load the latest background snapshot (if available).
    // This is a workaround for transparency regressions: instead of relying on a truly transparent
    // webview, we render a screenshot behind the selection UI.
    await setupSelectorSnapshotBackground();

    // Set up event listeners
    setupEventListeners();

    // Dim the entire screen until the user makes a selection.
    dimOverlayFull();

    // If a region is already set, preload it so the user can tweak it quickly.
    await restoreExistingSelection();

    // Ensure window has focus for keyboard events
    try {
        const currentWindow = window.__TAURI__.window.getCurrentWindow();
        await currentWindow.setFocus();
    } catch (e) {
        console.warn('Could not set window focus:', e);
    }

    // The body only accepts focus once it has a tabindex, so set that first.
    document.body.setAttribute('tabindex', '-1');
    if (!state.hasSelection) document.body.focus();
});

// =============================================================================
// DESKTOP SNAPSHOT BACKGROUND (TRANSPARENCY WORKAROUND)
// =============================================================================

async function setupSelectorSnapshotBackground() {
    if (!window.__TAURI__?.core?.invoke || !desktopSnapshot) return;

    // 1) Listen for new snapshots. The window is hidden and reused rather than
    //    reloaded, so a new snapshot is also the signal that it is being reopened.
    try {
        if (window.__TAURI__?.event?.listen) {
            await window.__TAURI__.event.listen('selector-background-snapshot', (event) => {
                applySelectorSnapshot(event.payload);
                resetSelection();
                restoreExistingSelection();
            });
        }
    } catch (e) {
        console.warn('Failed to listen for selector background snapshot events:', e);
    }

    // 2) Pull the most recent snapshot stored by the backend (covers first-load case).
    try {
        const snapshot = await window.__TAURI__.core.invoke('get_selector_snapshot');
        applySelectorSnapshot(snapshot);
    } catch (e) {
        console.warn('Failed to load selector background snapshot:', e);
    }
}

function applySelectorSnapshot(snapshot) {
    if (!desktopSnapshot) return;
    if (!snapshot?.dataUrl) return;

    desktopSnapshot.src = snapshot.dataUrl;
}

// =============================================================================
// DIM OVERLAY ("HOLE" AROUND SELECTION)
// =============================================================================

function dimOverlayFull() {
    if (!overlayTop || !overlayLeft || !overlayRight || !overlayBottom) return;

    overlayTop.style.top = '0px';
    overlayTop.style.left = '0px';
    overlayTop.style.width = '100%';
    overlayTop.style.height = '100%';

    // Collapse the other segments to avoid seams.
    overlayLeft.style.width = '0px';
    overlayLeft.style.height = '0px';
    overlayRight.style.width = '0px';
    overlayRight.style.height = '0px';
    overlayBottom.style.width = '0px';
    overlayBottom.style.height = '0px';
}

function dimOverlayWithHole(left, top, width, height) {
    if (!overlayTop || !overlayLeft || !overlayRight || !overlayBottom) return;

    const segments = buildDimOverlaySegments(
        { x: left, y: top, width, height },
        { width: window.innerWidth, height: window.innerHeight },
    );

    applyDimSegmentStyle(overlayTop, segments.top, true);
    applyDimSegmentStyle(overlayBottom, segments.bottom, true);
    applyDimSegmentStyle(overlayLeft, segments.left);
    applyDimSegmentStyle(overlayRight, segments.right);
}

function applyDimSegmentStyle(element, segment, fullWidth = false) {
    element.style.top = `${segment.top}px`;
    element.style.left = `${segment.left}px`;
    element.style.width = fullWidth ? '100%' : `${segment.width}px`;
    element.style.height = `${segment.height}px`;
}

// =============================================================================
// EVENT LISTENERS
// =============================================================================

function setupEventListeners() {
    // Mouse events for selection (on overlay area only)
    document.addEventListener('mousedown', handleMouseDown);
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    // Capture phase on the window, so the selector's keys work whichever
    // element holds focus.
    window.addEventListener('keydown', handleKeyDown, true);

    // Right-click cancels (common in region selectors)
    document.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        cancelSelection();
    });

    cancelBtn?.addEventListener('click', (e) => {
        e.stopPropagation();
        cancelSelection();
    });
    confirmBtn?.addEventListener('click', (e) => {
        e.stopPropagation();
        confirmSelection();
    });
    retryBtn?.addEventListener('click', (e) => {
        e.stopPropagation();
        resetSelection();
    });
}

// =============================================================================
// EXISTING REGION PRELOAD
// =============================================================================

async function restoreExistingSelection() {
    if (!window.__TAURI__?.core?.invoke) return;

    try {
        const existing = await window.__TAURI__.core.invoke('get_capture_region');
        if (!existing) return;
        if (!Number.isFinite(existing.width) || existing.width <= 0) return;
        if (!Number.isFinite(existing.height) || existing.height <= 0) return;

        showSelection({ ...existing });
    } catch (e) {
        console.warn('Failed to restore existing selection:', e);
    }
}

// =============================================================================
// PHASES
// =============================================================================

// The hint bar shows only what applies now: how to draw, how to adjust, or
// what went wrong.
function setPhase(phase, message = '') {
    document.body.dataset.phase = phase;
    if (errorText) errorText.textContent = message;
}

function setRegion(region) {
    state.region = region;
    state.startX = region.x;
    state.startY = region.y;
    state.currentX = region.x + region.width;
    state.currentY = region.y + region.height;
}

// Single entry to "a selection exists": a finished drag, a restored region,
// or a box placed from the keyboard.
function showSelection(region) {
    state.isSelecting = false;
    state.hasSelection = true;
    setRegion(region);

    selectionBox.classList.add('active', 'has-selection');
    actionButtons.classList.add('visible');
    setupDragAndResize();
    document.body.classList.add('selection-ready');
    setPhase('adjust');
    updateSelectionBox();
    confirmBtn?.focus({ preventScroll: true });
}

// =============================================================================
// MOUSE HANDLERS
// =============================================================================

function handleMouseDown(e) {
    // Clicks on the hint bar and the buttons are not the start of a drag.
    if (e.target.closest('.instructions, .action-buttons')) {
        return;
    }

    // If we already have a selection, ignore new mousedown
    if (state.hasSelection) {
        return;
    }

    e.preventDefault();

    // Start selection
    state.isSelecting = true;
    state.startX = e.screenX;
    state.startY = e.screenY;
    state.currentX = e.screenX;
    state.currentY = e.screenY;

    actionButtons.classList.remove('visible');
    selectionBox.classList.add('active');
    setPhase('draw');
    updateSelectionBox();
}

// Single owner of the document mousemove. Once a selection exists the same
// gesture means drag or resize instead of "draw a new rectangle", so the mode
// is branched here rather than by rebinding this handler later in the file.
function handleMouseMove(e) {
    if (isDragging) {
        e.preventDefault();
        handleDrag(e);
        return;
    }
    if (isResizing) {
        e.preventDefault();
        handleResize(e);
        return;
    }

    if (!state.isSelecting) return;

    e.preventDefault();
    state.currentX = e.screenX;
    state.currentY = e.screenY;

    updateSelectionBox();
}

// Single owner of the document mouseup, branching on the same three modes.
function handleMouseUp(e) {
    if (isDragging) {
        e.preventDefault();
        isDragging = false;
        dragRegionStart = null;
        return;
    }
    if (isResizing) {
        e.preventDefault();
        isResizing = false;
        resizeHandle = null;
        dragRegionStart = null;
        return;
    }

    if (!state.isSelecting) return;

    e.preventDefault();
    state.isSelecting = false;
    state.currentX = e.screenX;
    state.currentY = e.screenY;

    const region = calculateRegion();

    // Keep the rejected box on screen so the user can see what was too small.
    if (!meetsMinimumSelection(region)) {
        setPhase('error', TOO_SMALL_MESSAGE);
        return;
    }

    showSelection(region);
}

// =============================================================================
// KEYBOARD HANDLERS
// =============================================================================

function handleKeyDown(e) {
    if (e.key === 'Escape') {
        e.preventDefault();
        cancelSelection();
        return;
    }

    if (e.key === 'Enter') {
        // A focused button acts on Enter itself - Redraw must not confirm.
        if (e.target instanceof Element && e.target.closest('button')) return;
        e.preventDefault();
        if (state.hasSelection) {
            confirmSelection();
        } else if (!state.isSelecting) {
            showSelection(defaultSelectionRect({
                x: window.screenX,
                y: window.screenY,
                width: window.innerWidth,
                height: window.innerHeight,
            }));
        }
        return;
    }

    const delta = arrowDelta(e.key, e.ctrlKey);
    if (!delta || !state.hasSelection) return;
    e.preventDefault();

    // A focused handle resizes from its own edge; Shift resizes from the
    // bottom-right corner; otherwise the whole box moves.
    const handle = e.target instanceof HTMLElement ? e.target.dataset.position : undefined;
    if (handle || e.shiftKey) {
        setRegion(resizeRegion(state.region, handle || 'se', delta.dx, delta.dy, MIN_RESIZE_SIZE));
    } else {
        setRegion(moveRegion(state.region, delta.dx, delta.dy));
    }
    setPhase('adjust');
    updateSelectionBox();
}

// =============================================================================
// SELECTION BOX UPDATE
// =============================================================================

function updateSelectionBox() {
    const region = calculateRegion();

    // Use client coordinates for positioning the visual box.
    const clientRect = screenRectToClientRect(region, {
        x: window.screenX,
        y: window.screenY,
    });

    selectionBox.style.left = `${clientRect.left}px`;
    selectionBox.style.top = `${clientRect.top}px`;
    selectionBox.style.width = `${region.width}px`;
    selectionBox.style.height = `${region.height}px`;

    // Dim outside the selection for a "snipping tool" feel
    dimOverlayWithHole(clientRect.left, clientRect.top, region.width, region.height);

    if (actionButtons.classList.contains('visible')) {
        const top = actionBarTop(clientRect, window.innerHeight, actionButtons.offsetHeight, ACTION_BAR_GAP);
        actionButtons.style.top = `${top}px`;
    }
}

function calculateRegion() {
    // Calculate region using screen coordinates
    return selectionRectFromPoints(
        state.startX,
        state.startY,
        state.currentX,
        state.currentY,
    );
}

// =============================================================================
// SELECTION ACTIONS
// =============================================================================

async function confirmSelection() {
    if (!state.region || state.isSaving) return;
    state.isSaving = true;

    let scaleFactor = 1;
    try {
        const currentWindow = window.__TAURI__.window.getCurrentWindow();
        scaleFactor = await currentWindow.scaleFactor();
    } catch (e) {
        console.warn('Failed to read scale factor, defaulting to 1:', e);
    }

    // The selector tracks the region in screen coordinates (MouseEvent.screenX/Y), which matches
    // what the backend capture expects (logical/CSS pixels + a DPI scale factor).
    //
    // Add a small padding so OCR isn't overly sensitive to "tight" selections.
    const winLeft = Math.round(window.screenX || 0);
    const winTop = Math.round(window.screenY || 0);
    const winRight = winLeft + Math.round(window.innerWidth || 0);
    const winBottom = winTop + Math.round(window.innerHeight || 0);
    const regionData = buildCaptureRegionPayload(
        state.region,
        { left: winLeft, top: winTop, right: winRight, bottom: winBottom },
        scaleFactor,
    );

    try {
        await window.__TAURI__.core.invoke('set_capture_region', regionData);

        // Emit event to all windows
        try {
            await window.__TAURI__.event.emit('region-selected', regionData);
        } catch (emitError) {
            console.warn('Event emit warning:', emitError);
        }

        await closeWindow();
        resetSelection();
    } catch (error) {
        console.error('Failed to save region:', error);
        setPhase('error', SAVE_FAILED_MESSAGE);
    } finally {
        state.isSaving = false;
    }
}

function resetSelection() {
    state.isSelecting = false;
    state.hasSelection = false;
    state.region = null;

    selectionBox.classList.remove('active', 'has-selection');
    actionButtons.classList.remove('visible');

    document.body.classList.remove('selection-ready');
    setPhase('draw');
    dimOverlayFull();
    document.body.focus({ preventScroll: true });
}

async function cancelSelection() {
    // Close first, so the reset is never drawn in the moment before the window hides.
    await closeWindow();
    resetSelection();
}

async function closeWindow() {
    try {
        // Use Rust command to close window (more reliable than JS API)
        await window.__TAURI__.core.invoke('close_area_selector');
    } catch (error) {
        console.error('Failed to close window via command:', error);
        // Fallback: try JS API
        try {
            const currentWindow = window.__TAURI__.window.getCurrentWindow();
            await currentWindow.hide();
        } catch (e2) {
            console.error('Fallback also failed:', e2);
        }
    }
}

// =============================================================================
// DRAG AND RESIZE FUNCTIONALITY
// =============================================================================

// Extended state for drag/resize (added to global scope for simplicity)
let isDragging = false;
let isResizing = false;
let resizeHandle = null;
let dragStartX = 0;
let dragStartY = 0;
let dragRegionStart = null;
let dragResizeListenersAttached = false;

/**
 * Set up drag and resize handlers after selection is complete
 */
function setupDragAndResize() {
    if (dragResizeListenersAttached) return;
    dragResizeListenersAttached = true;

    // Handle resize handle mouse down
    const handles = selectionBox.querySelectorAll('.resize-handle');
    handles.forEach(handle => {
        handle.addEventListener('mousedown', handleResizeStart);
    });

    // Handle drag start on the selection box itself
    selectionBox.addEventListener('mousedown', handleDragStart);
}

/**
 * Start resizing when a handle is clicked
 */
function handleResizeStart(e) {
    if (!state.hasSelection) return;

    e.preventDefault();
    e.stopPropagation();

    isResizing = true;
    resizeHandle = e.target.dataset.position;
    dragStartX = e.screenX;
    dragStartY = e.screenY;
    dragRegionStart = { ...state.region };
    setPhase('adjust');
}

/**
 * Start dragging when selection box is clicked
 */
function handleDragStart(e) {
    if (!state.hasSelection) return;

    // Ignore if clicking on resize handle or button
    if (e.target.classList.contains('resize-handle') || e.target.closest('button')) {
        return;
    }

    e.preventDefault();
    e.stopPropagation();

    isDragging = true;
    dragStartX = e.screenX;
    dragStartY = e.screenY;
    dragRegionStart = { ...state.region };
    setPhase('adjust');
}

/**
 * Handle dragging the selection box
 */
function handleDrag(e) {
    if (!dragRegionStart) return;

    const deltaX = e.screenX - dragStartX;
    const deltaY = e.screenY - dragStartY;

    setRegion(moveRegion(dragRegionStart, deltaX, deltaY));
    updateSelectionBox();
}

/**
 * Handle resizing the selection box
 */
function handleResize(e) {
    if (!dragRegionStart || !resizeHandle) return;

    const deltaX = e.screenX - dragStartX;
    const deltaY = e.screenY - dragStartY;

    setRegion(resizeRegion(dragRegionStart, resizeHandle, deltaX, deltaY, MIN_RESIZE_SIZE));
    updateSelectionBox();
}
