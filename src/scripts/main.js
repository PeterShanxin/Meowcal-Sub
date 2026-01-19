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
    downloadInfo: null,
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

        // Load backend diagnostics
        await refreshTranslationDiagnostics();

        // Sync translation running state with backend
        // This fixes button state mismatch when page reloads while translation is running
        await syncTranslationState();

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
        const info = await TauriBridge.invoke('get_system_info');
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
        const settings = await TauriBridge.invoke('get_settings');
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

    document.getElementById('toggle-foundry-local').checked = config.enableFoundryLocal;
    document.getElementById('toggle-windows-ai').checked = config.enableWindowsAi;
    document.getElementById('toggle-offline-mt').checked = config.enableOfflineMt;
    document.getElementById('toggle-mock-fallback').checked = config.allowMockFallback;
    document.getElementById('offline-mt-path').value = config.offlineMt.binaryPath || '';

    // Load Foundry Local model selector
    const foundryModel = config.foundryLocal?.model || '';
    document.getElementById('foundry-local-model').value = foundryModel;

    // Populate Foundry Local models in background (don't block UI initialization)
    loadFoundryLocalModels().catch(error => {
        console.warn('Background model loading failed:', error);
    });
}

function normalizeTranslationConfig(translation) {
    const defaultConfig = {
        enableFoundryLocal: true,
        enableWindowsAi: true,
        enableOfflineMt: true,
        allowMockFallback: true,
        foundryLocal: {
            model: null,
            timeoutMs: 30000,
        },
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
        enableFoundryLocal: translation.enableFoundryLocal ?? defaultConfig.enableFoundryLocal,
        enableWindowsAi: translation.enableWindowsAi ?? defaultConfig.enableWindowsAi,
        enableOfflineMt: translation.enableOfflineMt ?? defaultConfig.enableOfflineMt,
        allowMockFallback: translation.allowMockFallback ?? defaultConfig.allowMockFallback,
        foundryLocal: {
            model: translation.foundryLocal?.model ?? defaultConfig.foundryLocal.model,
            timeoutMs: translation.foundryLocal?.timeoutMs ?? defaultConfig.foundryLocal.timeoutMs,
        },
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
    const foundryModel = document.getElementById('foundry-local-model').value.trim();

    translationConfig.enableFoundryLocal = document.getElementById('toggle-foundry-local').checked;
    translationConfig.enableWindowsAi = document.getElementById('toggle-windows-ai').checked;
    translationConfig.enableOfflineMt = document.getElementById('toggle-offline-mt').checked;
    translationConfig.allowMockFallback = document.getElementById('toggle-mock-fallback').checked;
    translationConfig.foundryLocal.model = foundryModel.length > 0 ? foundryModel : null;
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
        await TauriBridge.invoke('save_settings', { settings });
        appState.settings = settings;
        showToast('Settings saved!', 'success');
        console.log('Settings saved:', settings);
        await refreshTranslationDiagnostics();
    } catch (error) {
        console.error('Failed to save settings:', error);
        showToast('Failed to save settings', 'error');
    }
}

/**
 * Load available Foundry Local models and populate the dropdown
 */
async function loadFoundryLocalModels() {
    const select = document.getElementById('foundry-local-model');
    if (!select) return;

    try {
        const models = await TauriBridge.invoke('list_foundry_local_models');

        // Clear existing options except the first (Auto)
        while (select.options.length > 1) {
            select.remove(1);
        }

        // Add models to dropdown
        models.forEach(model => {
            const option = document.createElement('option');
            option.value = model;
            option.textContent = model;
            select.appendChild(option);
        });

        console.log(`Loaded ${models.length} Foundry Local models`);
    } catch (error) {
        console.warn('Failed to load Foundry Local models:', error);
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
    document.getElementById('btn-prepare-foundry')
        .addEventListener('click', handlePrepareFoundryLocal);

    // Auto-save when language settings change to ensure translation direction is persisted
    document.getElementById('source-language').addEventListener('change', async () => {
        console.log('Source language changed, auto-saving...');
        await saveSettings();
    });
    document.getElementById('target-language').addEventListener('change', async () => {
        console.log('Target language changed, auto-saving...');
        await saveSettings();
    });

    // Download translateLocally
    document.getElementById('btn-download-offline-mt')
        .addEventListener('click', handleOfflineMtDownload);

    // Windows AI diagnostics
    document.getElementById('btn-windows-ai-diagnostics')
        .addEventListener('click', handleWindowsAiDiagnostics);

    // Diagnostics modal close
    document.getElementById('btn-close-diagnostics')
        .addEventListener('click', closeDiagnosticsModal);
    document.getElementById('diagnostics-backdrop')
        .addEventListener('click', closeDiagnosticsModal);

    // translateLocally download modal actions
    document.getElementById('btn-cancel-download')
        .addEventListener('click', closeDownloadModal);
    document.getElementById('download-backdrop')
        .addEventListener('click', closeDownloadModal);
    document.getElementById('btn-confirm-download')
        .addEventListener('click', handleConfirmTranslateLocallyDownload);
    document.getElementById('download-option')
        .addEventListener('change', updateDownloadOptionNotes);
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
        await TauriBridge.invoke('open_area_selector');
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
            const region = await TauriBridge.invoke('get_capture_region');

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
    }, 250);
}

/**
 * Set up listener for region selection from the selector window
 */
async function setupRegionSelectedListener() {
    try {
        const unlisten = await TauriBridge.event.listen('region-selected', (event) => {
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
        await TauriBridge.event.listen('translation-update', (event) => {
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
        const diagnostics = await TauriBridge.invoke('get_translation_diagnostics');
        updateBackendStatusUI(diagnostics);
        await autoDetectOfflineMtPath();
        if (document.getElementById('toggle-foundry-local')?.checked) {
            await loadFoundryLocalModels();
        }
    } catch (error) {
        console.error('Failed to load backend diagnostics:', error);
        container.innerHTML = '<div class="backend-status-empty">Failed to load backend status.</div>';
    }
}

/**
 * Attempt to prepare Foundry Local (start service if needed)
 */
async function handlePrepareFoundryLocal() {
    const button = document.getElementById('btn-prepare-foundry');
    if (!button) {
        return;
    }

    button.disabled = true;
    const originalLabel = button.textContent;
    button.textContent = 'Preparing...';

    try {
        const status = await TauriBridge.invoke('prepare_foundry_local');
        await refreshTranslationDiagnostics();

        if (status?.serviceRunning) {
            if (status.models && status.models.length > 0) {
                showToast('Foundry Local is running', 'success');
            } else {
                showToast('Foundry Local started. No models cached yet.', 'warning');
            }
            await loadFoundryLocalModels();
        } else {
            const note = status?.notes ||
                'Foundry Local not available. Install via: winget install Microsoft.FoundryLocal';
            showToast(note, 'warning');
        }
    } catch (error) {
        console.error('Failed to prepare Foundry Local:', error);
        const message = error?.message ? error.message : String(error);
        showToast(`Failed to prepare Foundry Local: ${message}`, 'error');
    } finally {
        button.disabled = false;
        button.textContent = originalLabel;
    }
}

function backendIdKey(id) {
    switch (id) {
        case 'foundryLocal':
            return 'foundry_local';
        case 'windowsAi':
            return 'windows_ai';
        case 'offlineMt':
            return 'offline_mt';
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

    // Update new UI elements
    updateFoundryStatusInline(diagnostics);
    updateOfflineMtStatusInline(diagnostics);
    updateWindowsAiStatusInline(diagnostics);
    updateStatusSummary(diagnostics);
}

/**
 * Update Foundry Local status inline display
 */
function updateFoundryStatusInline(diagnostics) {
    const statusEl = document.getElementById('foundry-status');
    if (!statusEl || !diagnostics.backends) return;

    const foundry = diagnostics.backends.find(b => b.id === 'foundryLocal');
    if (!foundry) {
        statusEl.innerHTML = '<span class="status-text">Foundry Local not found</span>';
        return;
    }

    let readyState = foundry.readyState;
    if (!foundry.available && readyState === 'ready') {
        readyState = 'notReady';
    }

    const statusInfo = formatReadyState(readyState);
    const statusClass = statusInfo.className;
    const pill = `<span class="status-pill ${statusClass}">● ${statusInfo.label.toUpperCase()}</span>`;

    statusEl.innerHTML = `
        ${pill}
        <span class="status-text">${foundry.notes}</span>
    `;
}

/**
 * Update Offline MT status inline display
 */
function updateOfflineMtStatusInline(diagnostics) {
    const statusEl = document.getElementById('offline-mt-status');
    if (!statusEl || !diagnostics.backends) return;

    const offline = diagnostics.backends.find(b => b.id === 'offlineMt');
    if (!offline) {
        statusEl.innerHTML = '<span class="status-text">Offline MT not found</span>';
        return;
    }

    let readyState = offline.readyState;
    if (!offline.available && readyState === 'ready') {
        readyState = 'notReady';
    }

    const statusInfo = formatReadyState(readyState);
    const statusClass = statusInfo.className;
    const pill = `<span class="status-pill ${statusClass}">● ${statusInfo.label.toUpperCase()}</span>`;

    statusEl.innerHTML = `
        ${pill}
        <span class="status-text">${offline.notes}</span>
    `;
}

/**
 * Update Windows AI status inline display
 */
function updateWindowsAiStatusInline(diagnostics) {
    const statusEl = document.getElementById('windows-ai-status');
    if (!statusEl || !diagnostics.backends) return;

    const windowsAi = diagnostics.backends.find(b => b.id === 'windowsAi');
    if (!windowsAi) {
        statusEl.innerHTML = '<span class="status-text">Windows AI not found</span>';
        return;
    }

    let readyState = windowsAi.readyState;
    if (!windowsAi.available && readyState === 'ready') {
        readyState = 'notReady';
    }

    const statusInfo = formatReadyState(readyState);
    const statusClass = statusInfo.className;
    const pill = `<span class="status-pill ${statusClass}">● ${statusInfo.label.toUpperCase()}</span>`;

    statusEl.innerHTML = `
        ${pill}
        <span class="status-text">${windowsAi.notes}</span>
    `;
}

/**
 * Update status summary (X/Y ready)
 */
function updateStatusSummary(diagnostics) {
    const summaryEl = document.getElementById('status-summary');
    if (!summaryEl || !diagnostics.backends) return;

    const totalBackends = diagnostics.backends.length;
    const readyBackends = diagnostics.backends.filter(b => b.readyState === 'ready').length;

    summaryEl.textContent = `${readyBackends}/${totalBackends} Ready`;
}

async function handleOfflineMtDownload() {
    try {
        const info = await loadTranslateLocallyDownloadInfo();
        if (!info) {
            return;
        }
        populateDownloadModal(info);
        openDownloadModal();
    } catch (error) {
        console.error('Failed to prepare download modal:', error);
        showToast('Failed to load download options', 'error');
    }
}

async function autoDetectOfflineMtPath(force = false) {
    const input = document.getElementById('offline-mt-path');
    if (!input) {
        return;
    }

    if (!force && input.value.trim().length > 0) {
        return;
    }

    try {
        const detection = await TauriBridge.invoke('detect_offline_mt_binary');
        if (detection && detection.path) {
            input.value = detection.path;
            showToast(`Found translateLocally via ${detection.source}. Click Save Settings.`, 'success');
        } else if (force) {
            showToast('translateLocally not found yet. Install it and click Refresh.', 'warning');
        }
    } catch (error) {
        console.error('Offline MT detection failed:', error);
    }
}

async function handleWindowsAiDiagnostics() {
    try {
        const diagnostics = await TauriBridge.invoke('get_windows_ai_diagnostics');
        renderWindowsAiDiagnostics(diagnostics);
        openDiagnosticsModal();
    } catch (error) {
        console.error('Failed to load Windows AI diagnostics:', error);
        showToast('Failed to load Windows AI diagnostics', 'error');
    }
}

async function loadTranslateLocallyDownloadInfo() {
    if (appState.downloadInfo) {
        return appState.downloadInfo;
    }

    try {
        const info = await TauriBridge.invoke('get_translate_locally_download_info');
        appState.downloadInfo = info;
        return info;
    } catch (error) {
        console.error('Failed to load download info:', error);
        showToast('translateLocally download not available on this device', 'error');
        return null;
    }
}

function populateDownloadModal(info) {
    const recommendation = document.getElementById('download-recommendation');
    const optionSelect = document.getElementById('download-option');
    const installDir = document.getElementById('download-install-dir');

    if (!recommendation || !optionSelect || !installDir) {
        return;
    }

    optionSelect.innerHTML = '';
    info.options.forEach((option) => {
        const opt = document.createElement('option');
        opt.value = option.id;
        opt.textContent = option.label;
        optionSelect.appendChild(opt);
    });

    const recommendedId = info.recommendedId || (info.options[0] ? info.options[0].id : '');
    optionSelect.value = recommendedId;
    const recommendedOption = info.options.find((option) => option.id === recommendedId);
    recommendation.textContent = recommendedOption?.label || 'Recommended build';

    if (!installDir.value.trim()) {
        installDir.value = info.defaultInstallDir || '';
    }

    updateDownloadOptionNotes();
}

function updateDownloadOptionNotes() {
    const info = appState.downloadInfo;
    const optionSelect = document.getElementById('download-option');
    const notes = document.getElementById('download-option-notes');

    if (!info || !optionSelect || !notes) {
        return;
    }

    const selected = info.options.find((option) => option.id === optionSelect.value);
    notes.textContent = selected?.notes || '';

    const recommendation = document.getElementById('download-recommendation');
    if (recommendation && info.recommendedId) {
        const recommendedOption = info.options.find(
            (option) => option.id === info.recommendedId
        );
        recommendation.textContent = recommendedOption?.label || 'Recommended build';
    }
}

async function handleConfirmTranslateLocallyDownload() {
    const optionSelect = document.getElementById('download-option');
    const installDir = document.getElementById('download-install-dir');
    const confirmButton = document.getElementById('btn-confirm-download');

    if (!optionSelect || !installDir || !confirmButton) {
        return;
    }

    const optionId = optionSelect.value;
    const targetDir = installDir.value.trim();
    if (!targetDir) {
        showToast('Install folder is required', 'error');
        return;
    }

    confirmButton.disabled = true;
    const originalLabel = confirmButton.textContent;
    confirmButton.textContent = 'Downloading...';

    try {
        const result = await TauriBridge.invoke('download_translate_locally', {
            optionId,
            installDir: targetDir,
        });

        const offlineInput = document.getElementById('offline-mt-path');
        if (offlineInput) {
            offlineInput.value = result.path;
        }

        if (appState.settings?.translation?.offlineMt) {
            appState.settings.translation.offlineMt.binaryPath = result.path;
        }

        showToast(result.notes || 'translateLocally downloaded', 'success');
        closeDownloadModal();
        await refreshTranslationDiagnostics();
    } catch (error) {
        console.error('Download failed:', error);
        showToast(`Download failed: ${String(error)}`, 'error');
    } finally {
        confirmButton.disabled = false;
        confirmButton.textContent = originalLabel;
    }
}

function openDownloadModal() {
    const modal = document.getElementById('download-modal');
    if (modal) {
        modal.classList.remove('hidden');
    }
}

function closeDownloadModal() {
    const modal = document.getElementById('download-modal');
    if (modal) {
        modal.classList.add('hidden');
    }
}

function renderWindowsAiDiagnostics(diagnostics) {
    const container = document.getElementById('windows-ai-diagnostics');
    if (!container) {
        return;
    }

    container.innerHTML = '';

    if (!diagnostics) {
        container.textContent = 'No diagnostics available.';
        return;
    }

    const readyState = formatReadyState(diagnostics.readyState);
    const readyClass = diagnostics.readyState === 'ready'
        ? 'ok'
        : diagnostics.readyState === 'notReady'
            ? 'warn'
            : 'blocked';
    container.appendChild(
        createDiagnosticItem('Ready State', readyState.label, diagnostics.notes, readyClass)
    );

    const runtimeDetail = diagnostics.runtimeClassPresent
        ? 'LanguageModel runtime class detected.'
        : 'LanguageModel runtime class not registered.';
    container.appendChild(
        createDiagnosticItem(
            'Runtime Class',
            diagnostics.runtimeClassPresent ? 'OK' : 'Blocked',
            runtimeDetail,
            diagnostics.runtimeClassPresent ? 'ok' : 'blocked'
        )
    );

    const bindingsDetail = diagnostics.bindingsEnabled
        ? 'Bindings enabled.'
        : 'Enable feature windows_ai and add WinAppSDK bindings.';
    container.appendChild(
        createDiagnosticItem(
            'Bindings',
            diagnostics.bindingsEnabled ? 'OK' : 'Blocked',
            bindingsDetail,
            diagnostics.bindingsEnabled ? 'ok' : 'blocked'
        )
    );

    const packagingDetail = diagnostics.packagingNote || 'Packaging status unknown.';
    container.appendChild(
        createDiagnosticItem(
            'Packaging',
            diagnostics.packaged ? 'OK' : 'Blocked',
            packagingDetail,
            diagnostics.packaged ? 'ok' : 'blocked'
        )
    );

    const capabilityDetail = diagnostics.capabilityNote || 'Capability status unknown.';
    container.appendChild(
        createDiagnosticItem(
            'Capability',
            diagnostics.packaged ? 'Warn' : 'Blocked',
            capabilityDetail,
            diagnostics.packaged ? 'warn' : 'blocked'
        )
    );
}

function createDiagnosticItem(label, status, detail, statusClass) {
    const item = document.createElement('div');
    item.className = 'diag-item';

    const header = document.createElement('div');
    header.className = 'diag-header';

    const title = document.createElement('span');
    title.className = 'diag-label';
    title.textContent = label;

    const badge = document.createElement('span');
    badge.className = `diag-status ${statusClass}`;
    badge.textContent = status;

    header.appendChild(title);
    header.appendChild(badge);

    const body = document.createElement('div');
    body.textContent = detail;

    item.appendChild(header);
    item.appendChild(body);
    return item;
}

function openDiagnosticsModal() {
    const modal = document.getElementById('diagnostics-modal');
    if (modal) {
        modal.classList.remove('hidden');
    }
}

function closeDiagnosticsModal() {
    const modal = document.getElementById('diagnostics-modal');
    if (modal) {
        modal.classList.add('hidden');
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

        await TauriBridge.event.listen('capture-status', (event) => {
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
// STATE SYNC
// =============================================================================

/**
 * Sync translation running state with backend
 * Called on initialization to fix button state mismatch after page reload
 */
async function syncTranslationState() {
    try {
        const isRunning = await TauriBridge.invoke('is_translation_running');
        console.log('Translation running state from backend:', isRunning);

        appState.isRunning = isRunning;

        if (isRunning) {
            // Update UI to show Stop button
            document.getElementById('btn-start').style.display = 'none';
            document.getElementById('btn-stop').style.display = 'flex';
            document.getElementById('btn-stop').disabled = false;
            updateStatus('running', 'Translating...');
        } else {
            // Update UI to show Start button
            document.getElementById('btn-stop').style.display = 'none';
            document.getElementById('btn-start').style.display = 'flex';
            // Start button enabled state depends on region being set
            if (appState.captureRegion) {
                document.getElementById('btn-start').disabled = false;
            }
        }
    } catch (error) {
        console.warn('Failed to sync translation state:', error);
        // Assume not running if we can't check
        appState.isRunning = false;
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
        await TauriBridge.invoke('start_translation');
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
        await TauriBridge.invoke('stop_translation');
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
 * @param {'success' | 'error' | 'warning'} type
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
