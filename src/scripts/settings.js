// =============================================================================
// SETTINGS.JS - Settings Panel Logic
// =============================================================================
// Additional JavaScript for the settings panel.
// This file handles settings-specific UI interactions.
// =============================================================================

// This file exists for future expansion of settings functionality
// Current settings logic is handled in main.js

console.log('Settings script loaded');

// =============================================================================
// ADVANCED SETTINGS TOGGLE
// =============================================================================

/**
 * Toggle advanced settings visibility
 */
function toggleAdvancedSettings() {
    const toggle = document.querySelector('.advanced-toggle');
    const content = document.querySelector('.advanced-content');

    if (toggle && content) {
        toggle.classList.toggle('open');
        content.classList.toggle('open');
    }
}

// =============================================================================
// OVERLAY PREVIEW
// =============================================================================

/**
 * Show a preview of the overlay with current settings
 */
function previewOverlay() {
    // TODO: Create a temporary overlay to show how it will look
    console.log('Preview overlay clicked');
    window.MeowcalSub?.showToast('Overlay preview coming soon!', 'warning');
}

// =============================================================================
// KEYBOARD SHORTCUTS
// =============================================================================

// Listen for keyboard shortcuts
document.addEventListener('keydown', (e) => {
    // Escape to stop translation
    if (e.key === 'Escape' && window.MeowcalSub?.appState?.isRunning) {
        document.getElementById('btn-stop')?.click();
    }
});

// =============================================================================
// EXPORTS
// =============================================================================

window.MeowcalSubSettings = {
    toggleAdvancedSettings,
    previewOverlay,
};
