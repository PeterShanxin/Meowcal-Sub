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

        // Set up listener for region selection events
        await setupRegionSelectedListener();

        // Set up listener for translation results
        await setupTranslationUpdateListener();

        // Set up listener for capture status (fallback/error notifications)
        await setupCaptureStatusListener();

        // Register Edge Translator bridge (experimental)
        await registerEdgeTranslatorBridge();

        // Load backend diagnostics
        await refreshTranslationDiagnostics();

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
 * Register the Edge Translator bridge (if available)
 */
async function registerEdgeTranslatorBridge() {
    if (!window.MeowcalEdgeTranslator ||
        typeof window.MeowcalEdgeTranslator.registerEdgeTranslatorBridge !== 'function') {
        return;
    }

    try {
        await window.MeowcalEdgeTranslator.registerEdgeTranslatorBridge();
    } catch (error) {
        console.warn('Edge Translator bridge registration failed:', error);
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

        applyTranslationSettings(settings.translation);

        if (settings.lastCaptureRegion) {
            const region = settings.lastCaptureRegion;
            appState.captureRegion = region;
            document.getElementById('region-preview').style.display = 'block';
            document.getElementById('region-coords').textContent =
                `Position: (${region.x}, ${region.y})`;
            document.getElementById('region-size').textContent =
                `Size: ${region.width} × ${region.height}`;
            document.getElementById('btn-start').disabled = false;
        }

        console.log('Settings loaded:', settings);
    } catch (error) {
        console.error('Failed to load settings:', error);
        // Use defaults if loading fails
    }
}

function applyTranslationSettings(translation) {
    const config = normalizeTranslationConfig(translation);

    document.getElementById('backend-preference').value = config.preferredBackend;
    document.getElementById('toggle-windows-ai').checked = config.enableWindowsAi;
    document.getElementById('toggle-offline-mt').checked = config.enableOfflineMt;
    document.getElementById('toggle-edge-translator').checked = config.enableEdgeTranslator;
    document.getElementById('toggle-mock-fallback').checked = config.allowMockFallback;
    document.getElementById('offline-mt-path').value = config.offlineMt.binaryPath || '';
}

function normalizeTranslationConfig(translation) {
    const defaultConfig = {
        preferredBackend: 'auto',
        enableWindowsAi: true,
        enableOfflineMt: true,
        enableEdgeTranslator: false,
        allowMockFallback: true,
        offlineMt: {
            binaryPath: null,
            timeoutMs: 3000,
            maxChunkChars: 500,
        },
    };

    if (!translation) {
        return defaultConfig;
    }

    return {
        preferredBackend: translation.preferredBackend || defaultConfig.preferredBackend,
        enableWindowsAi: translation.enableWindowsAi ?? defaultConfig.enableWindowsAi,
        enableOfflineMt: translation.enableOfflineMt ?? defaultConfig.enableOfflineMt,
        enableEdgeTranslator: translation.enableEdgeTranslator ?? defaultConfig.enableEdgeTranslator,
        allowMockFallback: translation.allowMockFallback ?? defaultConfig.allowMockFallback,
        offlineMt: {
            binaryPath: translation.offlineMt?.binaryPath ?? defaultConfig.offlineMt.binaryPath,
            timeoutMs: translation.offlineMt?.timeoutMs ?? defaultConfig.offlineMt.timeoutMs,
            maxChunkChars: translation.offlineMt?.maxChunkChars ?? defaultConfig.offlineMt.maxChunkChars,
        },
    };
}

/**
 * Save settings to Rust backend
 */
async function saveSettings() {
    const translationConfig = normalizeTranslationConfig(appState.settings?.translation);
    const offlineMtPath = document.getElementById('offline-mt-path').value.trim();

    translationConfig.preferredBackend = document.getElementById('backend-preference').value;
    translationConfig.enableWindowsAi = document.getElementById('toggle-windows-ai').checked;
    translationConfig.enableOfflineMt = document.getElementById('toggle-offline-mt').checked;
    translationConfig.enableEdgeTranslator = document.getElementById('toggle-edge-translator').checked;
    translationConfig.allowMockFallback = document.getElementById('toggle-mock-fallback').checked;
    translationConfig.offlineMt.binaryPath = offlineMtPath.length > 0 ? offlineMtPath : null;

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
        translation: translationConfig,
    };

    try {
        await window.__TAURI__.core.invoke('save_settings', { settings });
        appState.settings = settings;
        showToast('Settings saved!', 'success');
        console.log('Settings saved:', settings);
        await refreshTranslationDiagnostics();
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

    // Backend diagnostics refresh
    document.getElementById('btn-refresh-backends')
        .addEventListener('click', refreshTranslationDiagnostics);

    // Edge model preparation
    document.getElementById('btn-edge-model-download')
        .addEventListener('click', handleEdgeModelDownload);
}

// =============================================================================
// CAPTURE REGION
// =============================================================================

/**
 * Handle the Select Area button click
 */
async function handleSelectArea() {
    console.log('Select area clicked');

    try {
        // Open the fullscreen selector overlay
        await window.__TAURI__.core.invoke('open_area_selector');
        console.log('Selector window opened');

        // Start polling for region changes (fallback in case events don't work)
        startRegionPolling();
    } catch (error) {
        console.error('Failed to open area selector:', error);
        showToast('Failed to open selector: ' + error, 'error');
    }
}

/**
 * Poll for region updates as a fallback mechanism
 */
let pollingInterval = null;

function startRegionPolling() {
    // Stop any existing polling
    if (pollingInterval) {
        clearInterval(pollingInterval);
    }

    const previousRegion = appState.captureRegion;
    let attempts = 0;
    const maxAttempts = 100; // 10 seconds max

    pollingInterval = setInterval(async () => {
        attempts++;

        try {
            const region = await window.__TAURI__.core.invoke('get_capture_region');

            // Check if we got a new region
            if (region && (!previousRegion ||
                region.x !== previousRegion.x ||
                region.y !== previousRegion.y ||
                region.width !== previousRegion.width ||
                region.height !== previousRegion.height)) {

                console.log('Region detected via polling:', region);
                clearInterval(pollingInterval);
                pollingInterval = null;

                // Update state and UI
                appState.captureRegion = region;
                document.getElementById('region-preview').style.display = 'block';
                document.getElementById('region-coords').textContent = `Position: (${region.x}, ${region.y})`;
                document.getElementById('region-size').textContent = `Size: ${region.width} × ${region.height}`;
                document.getElementById('btn-start').disabled = false;
                showToast('Region selected!', 'success');
            }
        } catch (e) {
            // Ignore errors during polling
        }

        // Stop polling after max attempts
        if (attempts >= maxAttempts) {
            clearInterval(pollingInterval);
            pollingInterval = null;
        }
    }, 100);
}

/**
 * Set up listener for region selection from the selector window
 */
async function setupRegionSelectedListener() {
    try {
        const unlisten = await window.__TAURI__.event.listen('region-selected', (event) => {
            const region = event.payload;
            console.log('Region selected via event:', region);

            // Stop polling since we got the event
            if (pollingInterval) {
                clearInterval(pollingInterval);
                pollingInterval = null;
            }

            appState.captureRegion = region;

            // Update UI
            document.getElementById('region-preview').style.display = 'block';
            document.getElementById('region-coords').textContent = `Position: (${region.x}, ${region.y})`;
            document.getElementById('region-size').textContent = `Size: ${region.width} × ${region.height}`;

            // Enable start button
            document.getElementById('btn-start').disabled = false;

            showToast('Region selected!', 'success');
        });
        console.log('Region selected listener set up');
    } catch (error) {
        console.error('Failed to set up region listener:', error);
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

/**
 * Set up listener for translation updates from the Rust backend
 */
async function setupTranslationUpdateListener() {
    try {
        await window.__TAURI__.event.listen('translation-update', (event) => {
            const { original, translated, timestamp } = event.payload;
            console.log('🌐 Translation update:', {
                original,
                translated,
                timestamp: new Date(timestamp).toLocaleTimeString(),
            });

            // Update the app state with latest translation
            appState.lastTranslation = event.payload;

            // TODO: Display in overlay window
            // For now, just show in console
        });
        console.log('Translation update listener set up');
    } catch (error) {
        console.error('Failed to set up translation listener:', error);
    }
}

/**
 * Refresh backend diagnostics and update UI
 */
async function refreshTranslationDiagnostics() {
    const container = document.getElementById('backend-status');
    if (!container) {
        return;
    }

    try {
        const diagnostics = await window.__TAURI__.core.invoke('get_translation_diagnostics');
        updateBackendStatusUI(diagnostics);
    } catch (error) {
        console.error('Failed to load backend diagnostics:', error);
        container.innerHTML = '<div class="backend-status-empty">Failed to load backend status.</div>';
    }
}

function backendIdKey(id) {
    switch (id) {
        case 'windowsAi':
            return 'windows_ai';
        case 'offlineMt':
            return 'offline_mt';
        case 'edgeTranslator':
            return 'edge_translator';
        case 'mock':
            return 'mock';
        default:
            return id;
    }
}

function formatReadyState(readyState) {
    switch (readyState) {
        case 'ready':
            return { label: 'Ready', className: 'ready' };
        case 'notReady':
            return { label: 'Not Ready', className: 'not-ready' };
        case 'notSupported':
            return { label: 'Not Supported', className: 'not-supported' };
        case 'error':
            return { label: 'Error', className: 'error' };
        default:
            return { label: 'Unknown', className: 'error' };
    }
}

function updateBackendStatusUI(diagnostics) {
    const container = document.getElementById('backend-status');
    if (!container) {
        return;
    }

    container.innerHTML = '';

    if (!diagnostics || !diagnostics.backends || diagnostics.backends.length === 0) {
        container.innerHTML = '<div class="backend-status-empty">No backend data.</div>';
        return;
    }

    diagnostics.backends.forEach((backend) => {
        const row = document.createElement('div');
        row.className = 'backend-status-row';

        const header = document.createElement('div');
        header.className = 'backend-status-header';

        const name = document.createElement('span');
        name.className = 'backend-status-name';
        name.textContent = backend.name;

        const statusInfo = formatReadyState(backend.readyState);
        const status = document.createElement('span');
        status.className = `status-pill ${statusInfo.className}`;
        status.textContent = statusInfo.label;

        header.appendChild(name);
        header.appendChild(status);

        const notes = document.createElement('div');
        notes.className = 'backend-status-notes';

        const backendKey = backendIdKey(backend.id);
        const errorCode = diagnostics.lastErrorByBackend?.[backendKey];
        const latency = diagnostics.lastLatencyByBackend?.[backendKey];

        let extra = '';
        if (typeof latency === 'number') {
            extra = `Last latency: ${latency}ms.`;
        }
        if (errorCode) {
            extra = extra ? `${extra} Last error: ${errorCode}.` : `Last error: ${errorCode}.`;
        }

        notes.textContent = backend.notes || extra || 'No notes available.';

        row.appendChild(header);
        row.appendChild(notes);
        container.appendChild(row);
    });
}

async function handleEdgeModelDownload() {
    if (!window.MeowcalEdgeTranslator ||
        typeof window.MeowcalEdgeTranslator.prepareEdgeTranslator !== 'function') {
        showToast('Edge Translator API not available', 'error');
        return;
    }

    const sourceLanguage = document.getElementById('source-language').value;
    const targetLanguage = document.getElementById('target-language').value;

    try {
        const result = await window.MeowcalEdgeTranslator.prepareEdgeTranslator(
            sourceLanguage,
            targetLanguage
        );

        if (result.readyState === 'ready') {
            showToast('Edge model already available', 'success');
        } else if (result.readyState === 'notReady') {
            showToast(result.notes || 'Edge model download started', 'success');
        } else {
            showToast(result.notes || 'Edge model not available', 'error');
        }
        await refreshTranslationDiagnostics();
    } catch (error) {
        console.error('Edge model prepare failed:', error);
        showToast('Failed to prepare Edge model', 'error');
    }
}

/**
 * Set up listener for capture status events from the Rust backend
 * This notifies us if:
 * - We're using GDI fallback (video may not capture correctly)
 * - There's a capture error
 */
async function setupCaptureStatusListener() {
    try {
        let hasShownFallbackWarning = false;  // Only show once per session

        await window.__TAURI__.event.listen('capture-status', (event) => {
            const { usingFallback, message, isError } = event.payload;
            console.log('📸 Capture status:', event.payload);

            if (isError) {
                // Show error toast and update status
                showToast(message, 'error');
                updateStatus('error', 'Capture failed');
            } else if (usingFallback && !hasShownFallbackWarning) {
                // Show fallback warning once
                hasShownFallbackWarning = true;
                showToast('⚠️ Using GDI fallback - video content may not capture correctly', 'warning');
                console.warn('Graphics Capture not available, using GDI fallback');
            }
        });
        console.log('Capture status listener set up');
    } catch (error) {
        console.error('Failed to set up capture status listener:', error);
    }
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
