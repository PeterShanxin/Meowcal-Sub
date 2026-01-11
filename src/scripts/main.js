// =============================================================================
// MAIN.JS - Application Entry Point (JavaScript)
// =============================================================================
// This is the main JavaScript file that runs when the app loads.
// 
// It handles:
// 1. Initializing the UI
// 2. Communicating with the Rust backend via Tauri
// 3. Updating the UI based on app state
//
// TAURI BASICS:
// - `invoke('command_name', { args })` calls a Rust function
// - Results come back as Promises, so we use async/await
// - The `__TAURI__` global is provided by Tauri
// =============================================================================

// Wait for the page to fully load before running our code
document.addEventListener('DOMContentLoaded', () => {
    console.log('🐱 Meowcal Sub UI loaded!');
    initializeApp();
});

// =============================================================================
// GLOBAL STATE
// =============================================================================

// Keep track of the app's current state
const appState = {
    isRunning: false,
    captureRegion: null,
    settings: null,
    systemInfo: null,
};

// =============================================================================
// INITIALIZATION
// =============================================================================

/**
 * Initialize the application
 * This runs once when the page loads
 */
async function initializeApp() {
    try {
        // Load system info from Rust
        await loadSystemInfo();

        // Load saved settings
        await loadSettings();

        // Set up event listeners for buttons
        setupEventListeners();

        // Update status to ready
        updateStatus('ready', 'Ready');

        console.log('✅ App initialized successfully!');
    } catch (error) {
        console.error('❌ Failed to initialize app:', error);
        updateStatus('error', 'Initialization failed');
        showToast('Failed to initialize: ' + error.message, 'error');
    }
}

/**
 * Load system information from the Rust backend
 */
async function loadSystemInfo() {
    console.log('Loading system info...');

    try {
        // Call the Rust function 'get_system_info'
        const info = await window.__TAURI__.core.invoke('get_system_info');
        appState.systemInfo = info;

        // Update the UI with system info
        updateSystemInfoUI(info);

        console.log('System info:', info);
    } catch (error) {
        console.error('Failed to load system info:', error);
        throw error;
    }
}

/**
 * Update the system info card in the UI
 */
function updateSystemInfoUI(info) {
    // Platform
    const platformEl = document.getElementById('info-platform');
    platformEl.textContent = info.os.charAt(0).toUpperCase() + info.os.slice(1);

    // Architecture
    const archEl = document.getElementById('info-arch');
    archEl.textContent = info.arch;
    if (info.arch === 'aarch64') {
        archEl.classList.add('success');
        archEl.textContent = 'ARM64 ✓';
    }

    // Copilot+ PC
    const copilotEl = document.getElementById('info-copilot');
    if (info.is_copilot_plus) {
        copilotEl.textContent = 'Yes ✓';
        copilotEl.classList.add('success');
    } else {
        copilotEl.textContent = 'No';
        copilotEl.classList.add('warning');
    }

    // Windows OCR
    const ocrEl = document.getElementById('info-ocr');
    if (info.windows_ocr_available) {
        ocrEl.textContent = 'Available ✓';
        ocrEl.classList.add('success');
    } else {
        ocrEl.textContent = 'Not available';
        ocrEl.classList.add('error');
    }

    // Phi Silica
    const phiEl = document.getElementById('info-phi');
    if (info.phi_silica_available) {
        phiEl.textContent = 'Available ✓';
        phiEl.classList.add('success');
    } else {
        phiEl.textContent = 'Coming soon';
        phiEl.classList.add('warning');
    }
}

// =============================================================================
// SETTINGS
// =============================================================================

/**
 * Load settings from Rust backend
 */
async function loadSettings() {
    console.log('Loading settings...');

    try {
        const settings = await window.__TAURI__.core.invoke('get_settings');
        appState.settings = settings;

        // Update UI with loaded settings
        document.getElementById('source-language').value = settings.sourceLanguage;
        document.getElementById('target-language').value = settings.targetLanguage;
        document.getElementById('capture-interval').value = settings.captureIntervalMs;
        document.getElementById('interval-value').textContent = settings.captureIntervalMs;

        console.log('Settings loaded:', settings);
    } catch (error) {
        console.error('Failed to load settings:', error);
        // Use defaults if loading fails
    }
}

/**
 * Save settings to Rust backend
 */
async function saveSettings() {
    const settings = {
        sourceLanguage: document.getElementById('source-language').value,
        targetLanguage: document.getElementById('target-language').value,
        captureIntervalMs: parseInt(document.getElementById('capture-interval').value),
        overlay: appState.settings?.overlay || {
            fontSize: 24,
            fontFamily: 'Segoe UI',
            textColor: '#FFFFFF',
            backgroundColor: 'rgba(0, 0, 0, 0.75)',
            offsetY: 10,
            maxWidth: 0,
        },
        autoStart: false,
        minimizeToTray: true,
        startWithWindows: false,
    };

    try {
        await window.__TAURI__.core.invoke('save_settings', { settings });
        appState.settings = settings;
        showToast('Settings saved!', 'success');
        console.log('Settings saved:', settings);
    } catch (error) {
        console.error('Failed to save settings:', error);
        showToast('Failed to save settings', 'error');
    }
}

// =============================================================================
// EVENT LISTENERS
// =============================================================================

/**
 * Set up all button click handlers
 */
function setupEventListeners() {
    // Select Area button
    document.getElementById('btn-select-area').addEventListener('click', handleSelectArea);

    // Start/Stop buttons
    document.getElementById('btn-start').addEventListener('click', handleStartTranslation);
    document.getElementById('btn-stop').addEventListener('click', handleStopTranslation);

    // Clear region button
    document.getElementById('btn-clear-region').addEventListener('click', handleClearRegion);

    // Save settings button
    document.getElementById('btn-save-settings').addEventListener('click', saveSettings);

    // Capture interval slider
    document.getElementById('capture-interval').addEventListener('input', (e) => {
        document.getElementById('interval-value').textContent = e.target.value;
    });
}

// =============================================================================
// CAPTURE REGION
// =============================================================================

/**
 * Handle the Select Area button click
 */
async function handleSelectArea() {
    console.log('Select area clicked');

    // TODO: Open a transparent fullscreen overlay for area selection
    // For now, we'll use a placeholder region
    showToast('Area selection coming soon! Using demo region...', 'warning');

    // Demo: Set a sample region (this would normally come from user selection)
    const region = {
        x: 100,
        y: 500,
        width: 800,
        height: 100,
    };

    try {
        await window.__TAURI__.core.invoke('set_capture_region', region);
        appState.captureRegion = region;

        // Update UI
        document.getElementById('region-preview').style.display = 'block';
        document.getElementById('region-coords').textContent = `Position: (${region.x}, ${region.y})`;
        document.getElementById('region-size').textContent = `Size: ${region.width} × ${region.height}`;

        // Enable start button
        document.getElementById('btn-start').disabled = false;

        showToast('Region selected!', 'success');
    } catch (error) {
        console.error('Failed to set region:', error);
        showToast('Failed to set region: ' + error, 'error');
    }
}

/**
 * Handle the Clear Region button click
 */
function handleClearRegion() {
    appState.captureRegion = null;
    document.getElementById('region-preview').style.display = 'none';
    document.getElementById('btn-start').disabled = true;
    showToast('Region cleared', 'success');
}

// =============================================================================
// TRANSLATION CONTROL
// =============================================================================

/**
 * Start translation
 */
async function handleStartTranslation() {
    console.log('Starting translation...');

    try {
        await window.__TAURI__.core.invoke('start_translation');
        appState.isRunning = true;

        // Update UI
        document.getElementById('btn-start').style.display = 'none';
        document.getElementById('btn-stop').style.display = 'flex';
        document.getElementById('btn-stop').disabled = false;
        updateStatus('running', 'Translating...');

        showToast('Translation started!', 'success');
    } catch (error) {
        console.error('Failed to start translation:', error);
        showToast('Failed to start: ' + error, 'error');
    }
}

/**
 * Stop translation
 */
async function handleStopTranslation() {
    console.log('Stopping translation...');

    try {
        await window.__TAURI__.core.invoke('stop_translation');
        appState.isRunning = false;

        // Update UI
        document.getElementById('btn-stop').style.display = 'none';
        document.getElementById('btn-start').style.display = 'flex';
        updateStatus('ready', 'Ready');

        showToast('Translation stopped', 'success');
    } catch (error) {
        console.error('Failed to stop translation:', error);
        showToast('Failed to stop: ' + error, 'error');
    }
}

// =============================================================================
// UI HELPERS
// =============================================================================

/**
 * Update the status indicator
 * @param {'ready' | 'running' | 'error' | 'warning'} state
 * @param {string} text
 */
function updateStatus(state, text) {
    const statusDot = document.querySelector('.status-dot');
    const statusText = document.querySelector('.status-text');

    // Remove all state classes
    statusDot.classList.remove('ready', 'running', 'error');

    // Add the new state class
    if (state !== 'warning') {
        statusDot.classList.add(state);
    }

    statusText.textContent = text;
}

/**
 * Show a toast notification
 * @param {string} message
 * @param {'success' | 'error'} type
 */
function showToast(message, type = 'success') {
    // Remove existing toast if any
    const existingToast = document.querySelector('.toast');
    if (existingToast) {
        existingToast.remove();
    }

    // Create new toast
    const toast = document.createElement('div');
    toast.className = `toast ${type}`;
    toast.textContent = message;
    document.body.appendChild(toast);

    // Remove after 3 seconds
    setTimeout(() => {
        toast.remove();
    }, 3000);
}

// =============================================================================
// EXPORTS (for other scripts to use)
// =============================================================================

window.MeowcalSub = {
    appState,
    showToast,
    updateStatus,
};
