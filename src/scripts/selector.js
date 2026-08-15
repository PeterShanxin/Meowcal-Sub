// =============================================================================
// SELECTOR.JS - Area Selection Logic
// =============================================================================
// Handles click-and-drag selection for choosing the subtitle capture region.
//
// Flow:
// 1. User clicks anywhere to start selection
// 2. User drags to define the rectangle
// 3. On mouse up, show Confirm/Redraw buttons
// 4. User clicks Confirm to save, or Redraw to try again
// 5. Cancel button or ESC closes without saving
// =============================================================================

const {
    buildCaptureRegionPayload,
    buildDimOverlaySegments,
    meetsMinimumSelection,
    moveRegion,
    resizeRegion,
    screenRectToClientRect,
    selectionRectFromPoints,
} = window.SelectorGeometry;

// Selection state
const state = {
    isSelecting: false,
    hasSelection: false,
    startX: 0,
    startY: 0,
    currentX: 0,
    currentY: 0,
    region: null,
};

// DOM elements - will be set after DOM loads
let selectionBox = null;
let dimensionsDisplay = null;
let instructions = null;
let actionButtons = null;
let confirmBtn = null;
let retryBtn = null;
let cancelBtn = null;
let desktopSnapshot = null;
let overlayTop = null;
let overlayLeft = null;
let overlayRight = null;
let overlayBottom = null;

// =============================================================================
// INITIALIZATION
// =============================================================================

document.addEventListener('DOMContentLoaded', async () => {
    console.log('📐 Selector window loaded');

    // Force transparent background via WebView2 API (workaround for Tauri 2.0 transparency issues)
    // On Windows 8+, alpha=0 creates true transparency
    try {
        const currentWebview = window.__TAURI__.webview.getCurrentWebview();
        await currentWebview.setBackgroundColor([0, 0, 0, 0]);
        console.log('✅ Set webview background to transparent');
    } catch (e) {
        console.warn('Could not set transparent background via webview API:', e);
        // Fallback: try window API (WebviewWindow combines window + webview)
        try {
            const currentWindow = window.__TAURI__.window.getCurrentWindow();
            await currentWindow.setBackgroundColor([0, 0, 0, 0]);
            console.log('✅ Set window background to transparent (fallback)');
        } catch (e2) {
            console.warn('Could not set transparent background:', e2);
        }
    }

    // Get DOM elements
    selectionBox = document.getElementById('selection-box');
    dimensionsDisplay = document.getElementById('dimensions');
    instructions = document.getElementById('instructions');
    actionButtons = document.getElementById('action-buttons');
    confirmBtn = document.getElementById('confirm-btn');
    retryBtn = document.getElementById('retry-btn');
    cancelBtn = document.getElementById('cancel-btn');
    desktopSnapshot = document.getElementById('desktop-snapshot');
    overlayTop = document.getElementById('overlay-top');
    overlayLeft = document.getElementById('overlay-left');
    overlayRight = document.getElementById('overlay-right');
    overlayBottom = document.getElementById('overlay-bottom');

    if (!selectionBox || !dimensionsDisplay || !instructions) {
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
        console.log('Window focus set');
    } catch (e) {
        console.warn('Could not set window focus:', e);
    }

    // Focus body for keyboard events
    document.body.focus();
    document.body.setAttribute('tabindex', '0');
});

// =============================================================================
// DESKTOP SNAPSHOT BACKGROUND (TRANSPARENCY WORKAROUND)
// =============================================================================

async function setupSelectorSnapshotBackground() {
    if (!window.__TAURI__?.core?.invoke || !desktopSnapshot) return;

    // 1) Listen for new snapshots (when the selector is opened again without a reload).
    try {
        if (window.__TAURI__?.event?.listen) {
            await window.__TAURI__.event.listen('selector-background-snapshot', (event) => {
                applySelectorSnapshot(event.payload);
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
// PREMIUM DIM OVERLAY ("HOLE" AROUND SELECTION)
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

    // Keyboard events - multiple targets for reliability
    document.addEventListener('keydown', handleKeyDown, true);
    window.addEventListener('keydown', handleKeyDown, true);

    // Right-click cancels (common in region selectors)
    document.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        cancelSelection();
    });

    // Button click events
    if (cancelBtn) {
        cancelBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            console.log('Cancel button clicked');
            cancelSelection();
        });
    }

    if (confirmBtn) {
        confirmBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            console.log('Confirm button clicked');
            confirmSelection();
        });
    }

    if (retryBtn) {
        retryBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            console.log('Retry button clicked');
            resetSelection();
        });
    }

    console.log('Event listeners set up');
}

// =============================================================================
// EXISTING REGION PRELOAD
// =============================================================================

async function restoreExistingSelection() {
    if (!window.__TAURI__?.core?.invoke) return;
    if (!selectionBox || !instructions) return;

    try {
        const existing = await window.__TAURI__.core.invoke('get_capture_region');
        if (!existing) return;
        if (!Number.isFinite(existing.width) || existing.width <= 0) return;
        if (!Number.isFinite(existing.height) || existing.height <= 0) return;

        state.isSelecting = false;
        state.hasSelection = true;
        state.region = { ...existing };
        state.startX = existing.x;
        state.startY = existing.y;
        state.currentX = existing.x + existing.width;
        state.currentY = existing.y + existing.height;

        selectionBox.classList.add('active', 'has-selection');
        updateSelectionBox();
        showActionButtons();
        setupDragAndResize();

        document.body.classList.add('selection-ready');
        instructions.style.opacity = '0.4';

        console.log('Preloaded existing region:', existing);
    } catch (e) {
        console.warn('Failed to restore existing selection:', e);
    }
}

// =============================================================================
// MOUSE HANDLERS
// =============================================================================

function handleMouseDown(e) {
    // Ignore clicks on buttons
    if (e.target.closest('.cancel-btn') ||
        e.target.closest('.confirm-btn') ||
        e.target.closest('.retry-btn')) {
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

    // Hide action buttons if visible
    actionButtons.classList.remove('visible');

    // Show and position the selection box
    selectionBox.classList.add('active');
    selectionBox.classList.remove('complete');
    updateSelectionBox();

    // Fade instructions
    instructions.style.opacity = '0.6';

    console.log(`Selection started at (${state.startX}, ${state.startY})`);
}

function handleMouseMove(e) {
    if (!state.isSelecting) return;

    e.preventDefault();
    state.currentX = e.screenX;
    state.currentY = e.screenY;

    updateSelectionBox();
}

function handleMouseUp(e) {
    if (!state.isSelecting) return;

    e.preventDefault();
    state.isSelecting = false;
    state.currentX = e.screenX;
    state.currentY = e.screenY;

    // Calculate the final region
    const region = calculateRegion();

    // Validate minimum size
    if (!meetsMinimumSelection(region)) {
        console.log('Selection too small, resetting');
        resetSelection();
        return;
    }

    console.log(`Selection complete: (${region.x}, ${region.y}) ${region.width}×${region.height}`);

    // Store the region
    state.region = region;
    state.hasSelection = true;

    // Show action buttons
    showActionButtons();

    // Add has-selection class to enable drag/resize handles
    selectionBox.classList.add('has-selection');

    // Set up drag and resize handlers
    setupDragAndResize();

    // Update visual state
    document.body.classList.add('selection-ready');
    instructions.style.opacity = '0.4';
}

// =============================================================================
// KEYBOARD HANDLERS
// =============================================================================

function handleKeyDown(e) {
    console.log('Key pressed:', e.key);

    if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        console.log('ESC pressed');
        cancelSelection();
    } else if (e.key === 'Enter' && state.hasSelection) {
        e.preventDefault();
        e.stopPropagation();
        console.log('Enter pressed');
        confirmSelection();
    }
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
    const left = clientRect.left;
    const top = clientRect.top;

    // Position and size the selection box
    selectionBox.style.left = `${left}px`;
    selectionBox.style.top = `${top}px`;
    selectionBox.style.width = `${region.width}px`;
    selectionBox.style.height = `${region.height}px`;

    // Dim outside the selection for a "snipping tool" feel
    dimOverlayWithHole(left, top, region.width, region.height);

    // Update dimensions display
    dimensionsDisplay.textContent = `${region.width} × ${region.height}`;

    // Check if near bottom of screen (for dimensions positioning)
    if (top + region.height > window.innerHeight - 100) {
        selectionBox.classList.add('near-bottom');
    } else {
        selectionBox.classList.remove('near-bottom');
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
// ACTION BUTTONS
// =============================================================================

function showActionButtons() {
    actionButtons.classList.add('visible');
}

function hideActionButtons() {
    actionButtons.classList.remove('visible');
}

// =============================================================================
// SELECTION ACTIONS
// =============================================================================

async function confirmSelection() {
    if (!state.region) {
        console.error('No region to confirm');
        return;
    }

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

    console.log('Confirming region:', regionData);

    // Flash animation
    selectionBox.classList.add('complete');

    try {
        // Save the region via Tauri command
        await window.__TAURI__.core.invoke('set_capture_region', regionData);
        console.log('✅ Region saved to backend');

        // Emit event to all windows
        try {
            await window.__TAURI__.event.emit('region-selected', regionData);
            console.log('✅ Event emitted');
        } catch (emitError) {
            console.warn('Event emit warning:', emitError);
        }

        // Small delay for visual feedback
        await new Promise(resolve => setTimeout(resolve, 300));

        // Close this window
        await closeWindow();

    } catch (error) {
        console.error('Failed to save region:', error);
        alert('Failed to save region: ' + error);
    }
}

function resetSelection() {
    console.log('Resetting selection');

    // Reset state
    state.isSelecting = false;
    state.hasSelection = false;
    state.region = null;

    // Hide elements
    selectionBox.classList.remove('active', 'complete', 'has-selection');
    hideActionButtons();

    // Reset visual state
    document.body.classList.remove('selection-ready');
    instructions.style.opacity = '1';
    dimOverlayFull();
}

async function cancelSelection() {
    console.log('Cancelling selection');

    // Reset state
    state.isSelecting = false;
    state.hasSelection = false;
    state.region = null;

    // Close window without saving
    await closeWindow();
}

async function closeWindow() {
    console.log('Closing selector window via Rust command...');
    try {
        // Use Rust command to close window (more reliable than JS API)
        await window.__TAURI__.core.invoke('close_area_selector');
        console.log('Window close command sent');
    } catch (error) {
        console.error('Failed to close window via command:', error);
        // Fallback: try JS API
        try {
            const currentWindow = window.__TAURI__.window.getCurrentWindow();
            await currentWindow.hide();
            console.log('Window hidden via fallback');
        } catch (e2) {
            console.error('Fallback also failed:', e2);
        }
    }
}

// =============================================================================
// GLOBAL FALLBACK HANDLERS
// =============================================================================

// Global escape handler as ultimate fallback
window.onkeydown = function (e) {
    if (e.key === 'Escape') {
        console.log('Global ESC handler');
        cancelSelection();
        return false;
    }
    if (e.key === 'Enter' && state.hasSelection) {
        console.log('Global Enter handler');
        confirmSelection();
        return false;
    }
};

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

    console.log(`Resize started from ${resizeHandle} handle`);
}

/**
 * Start dragging when selection box is clicked
 */
function handleDragStart(e) {
    if (!state.hasSelection) return;

    // Ignore if clicking on resize handle, button, or dimensions
    if (e.target.classList.contains('resize-handle') ||
        e.target.classList.contains('dimensions') ||
        e.target.closest('button')) {
        return;
    }

    e.preventDefault();
    e.stopPropagation();

    isDragging = true;
    dragStartX = e.screenX;
    dragStartY = e.screenY;
    dragRegionStart = { ...state.region };

    console.log('Drag started');
}

// Override existing mousemove for drag/resize
const originalMouseMove = handleMouseMove;
handleMouseMove = function (e) {
    if (isDragging) {
        e.preventDefault();
        handleDrag(e);
    } else if (isResizing) {
        e.preventDefault();
        handleResize(e);
    } else {
        originalMouseMove(e);
    }
};

// Override existing mouseup for drag/resize
const originalMouseUp = handleMouseUp;
handleMouseUp = function (e) {
    if (isDragging) {
        e.preventDefault();
        isDragging = false;
        dragRegionStart = null;
        console.log('Drag ended');
    } else if (isResizing) {
        e.preventDefault();
        isResizing = false;
        resizeHandle = null;
        dragRegionStart = null;
        console.log('Resize ended');
    } else {
        originalMouseUp(e);
    }
};

/**
 * Handle dragging the selection box
 */
function handleDrag(e) {
    if (!dragRegionStart) return;

    const deltaX = e.screenX - dragStartX;
    const deltaY = e.screenY - dragStartY;

    // Update region position
    state.region = moveRegion(dragRegionStart, deltaX, deltaY);

    // Update state for visual update
    state.startX = state.region.x;
    state.startY = state.region.y;
    state.currentX = state.region.x + state.region.width;
    state.currentY = state.region.y + state.region.height;

    updateSelectionBox();
}

/**
 * Handle resizing the selection box
 */
function handleResize(e) {
    if (!dragRegionStart || !resizeHandle) return;

    const deltaX = e.screenX - dragStartX;
    const deltaY = e.screenY - dragStartY;

    const nextRegion = resizeRegion(dragRegionStart, resizeHandle, deltaX, deltaY);

    // Update region
    state.region = nextRegion;

    // Update state for visual update
    state.startX = nextRegion.x;
    state.startY = nextRegion.y;
    state.currentX = nextRegion.x + nextRegion.width;
    state.currentY = nextRegion.y + nextRegion.height;

    updateSelectionBox();
}
