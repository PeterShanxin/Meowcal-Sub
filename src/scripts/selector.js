const {
    actionBarTop,
    arrowDelta,
    buildCaptureRegionPayload,
    buildDimOverlaySegments,
    containRegion,
    defaultSelectionRect,
    meetsMinimumSelection,
    regionFits,
    selectionRectFromPoints,
} = window.SelectorGeometry;
// Shared with the overlay, which enforces a larger minimum rectangle.
const { moveRegion, resizeRegion } = window.RegionGeometry;
const MIN_RESIZE_SIZE = 30;
const ACTION_BAR_GAP = 12;

// Shown in the hint bar instead of a native dialog or a silently dropped drag (#76).
const TOO_SMALL_MESSAGE = 'Area too small. Drag across one full line of subtitles.';
const SAVE_FAILED_MESSAGE = 'Couldn’t save the area. Try again.';

const state = {
    isSelecting: false,
    hasSelection: false,
    isSaving: false,
    startX: 0,
    startY: 0,
    currentX: 0,
    currentY: 0,
    region: null,
    request: null,
    selectionId: null,
};

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

    await loadSelectionRequest();

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

    // A reused selector receives its background before the selector-opened event.
    try {
        if (window.__TAURI__?.event?.listen) {
            await window.__TAURI__.event.listen('selector-background-snapshot', (event) => {
                applySelectorSnapshot(event.payload);
            });
            await window.__TAURI__.event.listen('selector-opened', async () => {
                await loadSelectionRequest();
                resetSelection();
                await restoreExistingSelection();
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

async function loadSelectionRequest() {
    applySelectionRequest(await window.TauriBridge.invoke('get_area_selection_request'));
}

function applySelectionRequest(request) {
    state.request = request;
    const label = request?.startTranslation ? 'Confirm and start' : 'Use this area';
    document.querySelectorAll('[data-confirm-label]').forEach(element => { element.textContent = label; });
}

function applySelectorSnapshot(snapshot) {
    if (!desktopSnapshot) return;
    state.selectionId = snapshot?.selectionId ?? null;
    if (snapshot?.dataUrl) desktopSnapshot.src = snapshot.dataUrl;
    else desktopSnapshot.removeAttribute('src');
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
    setRegion(containRegion(region, selectorViewport()));

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

// All selector geometry is in local CSS pixels; the backend adds the display origin.
function selectorViewport() {
    return { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight };
}

function selectorPoint(event) {
    return {
        x: Math.max(0, Math.min(event.clientX, window.innerWidth)),
        y: Math.max(0, Math.min(event.clientY, window.innerHeight)),
    };
}

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
    const { x, y } = selectorPoint(e);
    state.startX = state.currentX = x;
    state.startY = state.currentY = y;

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
    const { x, y } = selectorPoint(e);
    state.currentX = x;
    state.currentY = y;

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
    const { x, y } = selectorPoint(e);
    state.currentX = x;
    state.currentY = y;

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

    const viewport = selectorViewport();
    if (e.key === 'Enter') {
        // A focused button acts on Enter itself - Redraw must not confirm.
        if (e.target instanceof Element && e.target.closest('button')) return;
        e.preventDefault();
        if (state.hasSelection) {
            confirmSelection();
        } else if (!state.isSelecting) {
            showSelection(defaultSelectionRect(viewport));
        }
        return;
    }

    const delta = arrowDelta(e.key, e.ctrlKey);
    if (!delta || !state.hasSelection) return;
    e.preventDefault();

    // A focused handle resizes from its own edge; Shift resizes from the
    // bottom-right corner; otherwise the whole box moves. Neither leaves the screen.
    const handle = e.target instanceof HTMLElement ? e.target.dataset.position : undefined;
    if (handle || e.shiftKey) {
        const resized = resizeRegion(state.region, handle || 'se', delta.dx, delta.dy, MIN_RESIZE_SIZE);
        if (regionFits(resized, viewport)) setRegion(resized);
    } else {
        setRegion(containRegion(moveRegion(state.region, delta.dx, delta.dy), viewport));
    }
    setPhase('adjust');
    updateSelectionBox();
}

// =============================================================================
// SELECTION BOX UPDATE
// =============================================================================

function updateSelectionBox() {
    const region = calculateRegion();

    selectionBox.style.left = `${region.x}px`;
    selectionBox.style.top = `${region.y}px`;
    selectionBox.style.width = `${region.width}px`;
    selectionBox.style.height = `${region.height}px`;

    // Dim outside the selection for a "snipping tool" feel
    dimOverlayWithHole(region.x, region.y, region.width, region.height);

    if (actionButtons.classList.contains('visible')) {
        const top = actionBarTop({ top: region.y, height: region.height }, window.innerHeight, actionButtons.offsetHeight, ACTION_BAR_GAP);
        actionButtons.style.top = `${top}px`;
    }
}

function calculateRegion() {
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

    const regionData = buildCaptureRegionPayload(
        state.region,
        { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight },
        scaleFactor,
    );

    try {
        if (state.request) {
            const { scaleFactor: scale, ...region } = regionData;
            await window.TauriBridge.invoke('complete_area_selection', {
                requestId: state.request.requestId, region, scaleFactor: scale, selectionId: state.selectionId,
            });
        } else {
            await window.__TAURI__.core.invoke('set_capture_region', { ...regionData, selectionId: state.selectionId });
            await closeWindow();
            await window.__TAURI__.event.emit('region-selected', regionData);
        }
        resetSelection();
    } catch (error) {
        console.error('Failed to save region:', error);
        setPhase('error', typeof error === 'string' ? error : SAVE_FAILED_MESSAGE);
    } finally {
        state.isSaving = false;
    }
}

function resetSelection() {
    isDragging = isResizing = false;
    resizeHandle = dragRegionStart = null;
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
    if (state.isSaving) return;
    state.isSaving = true;
    try {
        if (state.request) {
            await window.TauriBridge.invoke('complete_area_selection', {
                requestId: state.request.requestId, region: null,
            });
        } else await closeWindow();
        resetSelection();
    } catch (error) {
        console.error('Failed to cancel area selection:', error);
        setPhase('error', 'Couldn’t close the selector. Try again.');
    } finally {
        state.isSaving = false;
    }
}

async function closeWindow() {
    await window.TauriBridge.invoke('close_area_selector');
}

// =============================================================================
// DRAG AND RESIZE FUNCTIONALITY
// =============================================================================

let isDragging = false;
let isResizing = false;
let resizeHandle = null;
let dragStartX = 0;
let dragStartY = 0;
let dragRegionStart = null;
let dragResizeListenersAttached = false;

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

function handleResizeStart(e) {
    if (!state.hasSelection) return;

    e.preventDefault();
    e.stopPropagation();

    isResizing = true;
    resizeHandle = e.target.dataset.position;
    const point = selectorPoint(e);
    dragStartX = point.x;
    dragStartY = point.y;
    dragRegionStart = { ...state.region };
    setPhase('adjust');
}

function handleDragStart(e) {
    if (!state.hasSelection) return;

    // Ignore if clicking on resize handle or button
    if (e.target.classList.contains('resize-handle') || e.target.closest('button')) {
        return;
    }

    e.preventDefault();
    e.stopPropagation();

    isDragging = true;
    const point = selectorPoint(e);
    dragStartX = point.x;
    dragStartY = point.y;
    dragRegionStart = { ...state.region };
    setPhase('adjust');
}

function handleDrag(e) {
    if (!dragRegionStart) return;

    const point = selectorPoint(e);
    const deltaX = point.x - dragStartX;
    const deltaY = point.y - dragStartY;

    setRegion(containRegion(moveRegion(dragRegionStart, deltaX, deltaY), selectorViewport()));
    updateSelectionBox();
}

function handleResize(e) {
    if (!dragRegionStart || !resizeHandle) return;

    const point = selectorPoint(e);
    const deltaX = point.x - dragStartX;
    const deltaY = point.y - dragStartY;

    setRegion(containRegion(resizeRegion(dragRegionStart, resizeHandle, deltaX, deltaY, MIN_RESIZE_SIZE), selectorViewport()));
    updateSelectionBox();
}
