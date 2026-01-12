// =============================================================================
// OVERLAY.JS - Overlay Window Controller
// =============================================================================
// This script controls the overlay window which displays:
// 1. A border around the capture region
// 2. Translated subtitles below the capture region
//
// It listens for Tauri events to update position and content.
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
    region: null,    // { x, y, width, height }
    isVisible: false,
    currentText: '',
    debugMode: true,  // ENABLED for debugging
};

// =============================================================================
// INITIALIZATION
// =============================================================================

async function initOverlay() {
    console.log('🔧 Initializing overlay...');

    // Get DOM elements
    const captureFrame = document.getElementById('capture-frame');
    const subtitleContainer = document.getElementById('subtitle-container');
    const subtitleText = document.getElementById('subtitle-text');
    const debugInfo = document.getElementById('debug-info');
    const debugRegion = document.getElementById('debug-region');
    const debugStatus = document.getElementById('debug-status');

    // Update debug status
    if (debugStatus) {
        debugStatus.textContent = 'Status: setting up listeners...';
    }

    // Set up event listeners
    await setupEventListeners({
        captureFrame,
        subtitleContainer,
        subtitleText,
        debugRegion,
        debugStatus,
    });

    // FETCH INITIAL STATE
    // This fixes the race condition where overlay might miss the first event
    try {
        if (debugStatus) debugStatus.textContent = 'Status: fetching initial state...';

        const region = await window.__TAURI__.core.invoke('get_capture_region');
        if (region) {
            console.log('📍 Initial region found:', region);
            overlayState.region = region;
            overlayState.isVisible = true;  // If we have a region, translation is active
            updateCaptureFrame(captureFrame, region);
            updateSubtitlePosition(subtitleContainer, region);

            // Explicitly show the frame - we may have missed the visibility event due to race condition
            captureFrame.classList.remove('hidden');
            captureFrame.classList.add('visible');

            if (debugRegion) {
                debugRegion.textContent = `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
            }
            if (debugStatus) {
                debugStatus.textContent = 'Status: region loaded, waiting for text...';
            }
        } else {
            console.log('📍 No initial region set');
        }
    } catch (e) {
        console.error('Failed to get initial region:', e);
    }

    if (debugStatus) {
        debugStatus.textContent = 'Status: ready, waiting for events...';
    }

    console.log('✅ Overlay initialized');
}

// =============================================================================
// EVENT LISTENERS
// =============================================================================

async function setupEventListeners(elements) {
    const { captureFrame, subtitleContainer, subtitleText, debugRegion, debugStatus } = elements;

    try {
        // Listen for region updates (when capture region changes)
        await window.__TAURI__.event.listen('overlay-update-region', (event) => {
            const region = event.payload;
            console.log('📍 Region update:', region);

            overlayState.region = region;
            updateCaptureFrame(captureFrame, region);
            updateSubtitlePosition(subtitleContainer, region);

            // Always update debug info
            if (debugRegion) {
                debugRegion.textContent = `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
            }
        });

        // Listen for translation updates
        await window.__TAURI__.event.listen('translation-update', (event) => {
            const { original, translated, timestamp } = event.payload;
            console.log('🌐 Translation:', translated);

            updateSubtitleText(subtitleText, translated, subtitleContainer);

            if (overlayState.debugMode) {
                debugStatus.textContent = `Last update: ${new Date(timestamp).toLocaleTimeString()}`;
            }
        });

        // Listen for overlay show/hide commands
        await window.__TAURI__.event.listen('overlay-visibility', async (event) => {
            const visible = event.payload;
            console.log('👁️ Visibility:', visible);

            overlayState.isVisible = visible;

            if (visible) {
                // Fetch current region since we may have missed the region event
                try {
                    const region = await window.__TAURI__.core.invoke('get_capture_region');
                    if (region) {
                        console.log('📍 Fetched region on visibility:', region);
                        overlayState.region = region;
                        updateCaptureFrame(captureFrame, region);
                        updateSubtitlePosition(subtitleContainer, region);

                        if (debugRegion) {
                            debugRegion.textContent = `Region: (${region.x}, ${region.y}) ${region.width}x${region.height}`;
                        }
                        if (debugStatus) {
                            debugStatus.textContent = 'Status: visible, waiting for text...';
                        }
                    }
                } catch (e) {
                    console.error('Failed to fetch region on visibility:', e);
                }

                captureFrame.classList.remove('hidden');
                captureFrame.classList.add('visible');
            } else {
                captureFrame.classList.add('hidden');
                captureFrame.classList.remove('visible');
                subtitleContainer.classList.add('hidden');
                subtitleContainer.classList.remove('visible');
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

/**
 * Update the capture frame position and size
 */
function updateCaptureFrame(frame, region) {
    if (!region) return;

    frame.style.left = `${region.x}px`;
    frame.style.top = `${region.y}px`;
    frame.style.width = `${region.width}px`;
    frame.style.height = `${region.height}px`;

    frame.classList.remove('hidden');
    frame.classList.add('visible');
}

/**
 * Update subtitle container position (below capture region)
 */
function updateSubtitlePosition(container, region) {
    if (!region) return;

    // Position below the capture region with some padding
    const padding = 10;
    const top = region.y + region.height + padding;

    // Center horizontally relative to the capture region
    const centerX = region.x + (region.width / 2);

    container.style.top = `${top}px`;
    container.style.left = `${centerX}px`;
    // transform: translateX(-50%) is in CSS to center it
}

/**
 * Update subtitle text with fade animation
 */
function updateSubtitleText(textElement, newText, container) {
    if (!newText || newText.trim() === '') {
        container.classList.add('hidden');
        container.classList.remove('visible');
        return;
    }

    // Skip if text hasn't changed
    if (newText === overlayState.currentText) {
        return;
    }

    overlayState.currentText = newText;

    // Fade in new text
    textElement.classList.remove('fade-in');
    // Force reflow to restart animation
    void textElement.offsetWidth;
    textElement.textContent = newText;
    textElement.classList.add('fade-in');

    // Show container
    container.classList.remove('hidden');
    container.classList.add('visible');
}

// =============================================================================
// EXPORTS
// =============================================================================

window.OverlayController = {
    state: overlayState,
};
