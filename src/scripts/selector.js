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

// =============================================================================
// INITIALIZATION
// =============================================================================

document.addEventListener('DOMContentLoaded', async () => {
    console.log('📐 Selector window loaded');

    // Get DOM elements
    selectionBox = document.getElementById('selection-box');
    dimensionsDisplay = document.getElementById('dimensions');
    instructions = document.getElementById('instructions');
    actionButtons = document.getElementById('action-buttons');
    confirmBtn = document.getElementById('confirm-btn');
    retryBtn = document.getElementById('retry-btn');
    cancelBtn = document.getElementById('cancel-btn');

    if (!selectionBox || !dimensionsDisplay || !instructions) {
        console.error('Failed to find required DOM elements');
        return;
    }

    // Set up event listeners
    setupEventListeners();

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
    if (region.width < 30 || region.height < 15) {
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

    // Use client coordinates for positioning the visual box
    const clientStartX = state.startX - window.screenX;
    const clientStartY = state.startY - window.screenY;
    const clientCurrentX = state.currentX - window.screenX;
    const clientCurrentY = state.currentY - window.screenY;

    const left = Math.min(clientStartX, clientCurrentX);
    const top = Math.min(clientStartY, clientCurrentY);

    // Position and size the selection box
    selectionBox.style.left = `${left}px`;
    selectionBox.style.top = `${top}px`;
    selectionBox.style.width = `${region.width}px`;
    selectionBox.style.height = `${region.height}px`;

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
    const x = Math.min(state.startX, state.currentX);
    const y = Math.min(state.startY, state.currentY);
    const width = Math.abs(state.currentX - state.startX);
    const height = Math.abs(state.currentY - state.startY);

    return { x, y, width, height };
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

    const regionData = {
        x: Math.round(state.region.x),
        y: Math.round(state.region.y),
        width: Math.round(state.region.width),
        height: Math.round(state.region.height),
        scaleFactor: scaleFactor,
    };

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
    selectionBox.classList.remove('active', 'complete');
    hideActionButtons();

    // Reset visual state
    document.body.classList.remove('selection-ready');
    instructions.style.opacity = '1';
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
