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
    foundryAutoProbe: {
        inFlight: false,
        lastAttemptMs: 0,
        timerId: null,
        attempts: 0,
    },
    foundryStatus: {
        last: null,
        lastCheckedMs: 0,
    },
    ocrLanguages: null, // Set of installed BCP-47 tags, populated by loadOcrLanguages()
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

        // Load OCR language availability and update dropdown
        await loadOcrLanguages();

        // Set up event listeners for buttons
        setupEventListeners();

        // Set up listener for region selection events
        await setupRegionSelectedListener();

        // Set up listener for translation results
        await setupTranslationUpdateListener();

        // Set up listener for capture status (fallback/error notifications)
        await setupCaptureStatusListener();

        // Load backend diagnostics (fast snapshot)
        await refreshTranslationDiagnostics();

        // Paint Foundry Local status quickly, then auto-probe readiness in the background.
        await refreshFoundryStatus({ probe: false, reason: 'startup' });
        scheduleFoundryAutoProbe();

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
        applyOverlaySettings(settings.overlay);
        applyOcrSettings(settings.translation);

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

// =============================================================================
// OCR LANGUAGE MANAGEMENT
// =============================================================================

const KNOWN_SOURCE_LANGUAGES = [
    { value: 'en-US', label: 'English (US)' },
    { value: 'ja-JP', label: 'Japanese' },
    { value: 'zh-CN', label: 'Chinese (Simplified)' },
    { value: 'zh-TW', label: 'Chinese (Traditional)' },
    { value: 'ko-KR', label: 'Korean' },
    { value: 'es-ES', label: 'Spanish' },
    { value: 'fr-FR', label: 'French' },
    { value: 'de-DE', label: 'German' },
];

/**
 * Load installed OCR languages and update the source language dropdown
 */
async function loadOcrLanguages() {
    try {
        const langs = await TauriBridge.invoke('get_ocr_languages');
        appState.ocrLanguages = new Set(langs);
        console.log('OCR languages installed:', langs);
    } catch (error) {
        console.warn('Could not load OCR languages:', error);
        // Treat failure as "unknown" — still populate from KNOWN_SOURCE_LANGUAGES
        appState.ocrLanguages = new Set();
    }

    // Use saved setting as the selected value, since the HTML only has en-US
    // as a static fallback and loadSettings() may have set a different value.
    const currentValue = appState.settings?.sourceLanguage
        || document.getElementById('source-language').value;
    populateSourceLanguageDropdown(appState.ocrLanguages, currentValue);
    checkOcrLanguageWarning(currentValue);
}

/**
 * Rebuild the source language dropdown, marking uninstalled languages
 */
function populateSourceLanguageDropdown(installedSet, currentValue) {
    const select = document.getElementById('source-language');
    select.innerHTML = '';

    for (const lang of KNOWN_SOURCE_LANGUAGES) {
        const option = document.createElement('option');
        option.value = lang.value;
        const installed = installedSet.has(lang.value);
        if (installed) {
            option.textContent = lang.label;
        } else {
            option.textContent = `${lang.label} \u2014 not installed`;
            option.dataset.notInstalled = 'true';
        }
        select.appendChild(option);
    }

    // Restore previously selected value
    select.value = currentValue;
    if (!select.value) {
        select.value = 'en-US';
    }
}

/**
 * Show or hide the OCR language warning based on the selected language
 */
function checkOcrLanguageWarning(selectedValue) {
    const warning = document.getElementById('ocr-lang-warning');
    const warningText = document.getElementById('ocr-lang-warning-text');
    const installBtn = document.getElementById('ocr-lang-install-btn');

    if (!warning || !appState.ocrLanguages) return;

    if (!appState.ocrLanguages.has(selectedValue)) {
        const langName = KNOWN_SOURCE_LANGUAGES.find(l => l.value === selectedValue)?.label || selectedValue;
        warningText.textContent = `${langName} OCR is not installed.`;
        installBtn.textContent = 'Install';
        installBtn.disabled = false;
        warning.style.display = 'flex';
    } else {
        warning.style.display = 'none';
    }
}

/**
 * Install the currently selected OCR language pack via elevated PowerShell
 */
async function installOcrLanguage() {
    const select = document.getElementById('source-language');
    const languageTag = select.value;
    const installBtn = document.getElementById('ocr-lang-install-btn');

    try {
        installBtn.textContent = 'Installing...';
        installBtn.disabled = true;

        await TauriBridge.invoke('install_ocr_language', { languageTag });

        // Wait briefly for Windows to register the new capability
        await new Promise(resolve => setTimeout(resolve, 2000));

        // Refresh the language list
        await loadOcrLanguages();

        if (appState.ocrLanguages.has(languageTag)) {
            showToast('OCR language pack installed successfully!', 'success');
        } else {
            showToast('Installation may have been cancelled or is still in progress.', 'warning');
            installBtn.textContent = 'Install';
            installBtn.disabled = false;
        }
    } catch (error) {
        console.error('Failed to install OCR language:', error);
        showToast('Failed to install language pack: ' + error.message, 'error');
        installBtn.textContent = 'Install';
        installBtn.disabled = false;
    }
}

function applyTranslationSettings(translation) {
    const config = normalizeTranslationConfig(translation);

    document.getElementById('toggle-foundry-local').checked = config.enableFoundryLocal;
    document.getElementById('toggle-windows-ai').checked = config.enableWindowsAi;
    document.getElementById('toggle-offline-mt').checked = config.enableOfflineMt;
    document.getElementById('toggle-mock-fallback').checked = config.allowMockFallback;
    document.getElementById('context-level').value = config.contextLevel;
    document.getElementById('context-recent-count').value = config.contextRecentCount;
    document.getElementById('context-budget-percent').value = config.contextBudgetPercent;
    document.getElementById('context-summary-cooldown-ms').value = config.contextSummaryCooldownMs;
    document.getElementById('prompt-max-source-chars').value = config.promptMaxSourceChars;
    document.getElementById('prompt-max-context-chars').value = config.promptMaxContextChars;
    document.getElementById('context-buffer-size').value = config.contextBufferSize;
    document.getElementById('context-reset-gap-ms').value = config.contextResetGapMs;
    document.getElementById('offline-mt-path').value = config.offlineMt.binaryPath || '';

    syncContextControls();

    // Load Foundry Local model selector
    const foundryModel = config.foundryLocal?.model || '';
    const modelSelect = document.getElementById('foundry-local-model');
    if (modelSelect) {
        if (foundryModel) {
            // Ensure the selected model is visible immediately even before the list loads
            const hasOption = Array.from(modelSelect.options).some(opt => opt.value === foundryModel);
            if (!hasOption) {
                const option = document.createElement('option');
                option.value = foundryModel;
                option.textContent = `${foundryModel} (selected)`;
                modelSelect.appendChild(option);
            }
            modelSelect.value = foundryModel;
        } else {
            modelSelect.value = '';
        }
    }

    // Populate Foundry Local models in background (don't block UI initialization)
    loadFoundryLocalModels(foundryModel).catch(error => {
        console.warn('Background model loading failed:', error);
    });
}

function applyOverlaySettings(overlay) {
    const defaults = {
        fontSize: 24,
        fontFamily: 'Segoe UI',
        textColor: '#FFFFFF',
        backgroundColor: 'rgba(0, 0, 0, 0.75)',
        offsetY: 10,
        maxWidth: 0,
        showDiagnostics: false,
    };

    const config = overlay || defaults;

    // Font size
    const fontSizeSlider = document.getElementById('overlay-font-size');
    const fontSizeValue = document.getElementById('overlay-font-size-value');
    if (fontSizeSlider) fontSizeSlider.value = config.fontSize || defaults.fontSize;
    if (fontSizeValue) fontSizeValue.textContent = config.fontSize || defaults.fontSize;

    // Font family
    const fontFamily = document.getElementById('overlay-font-family');
    if (fontFamily) fontFamily.value = config.fontFamily || defaults.fontFamily;

    // Text color
    const textColor = document.getElementById('overlay-text-color');
    if (textColor) {
        // Extract hex color from value (might be named color or hex)
        const hexColor = config.textColor?.startsWith('#') ? config.textColor : '#FFFFFF';
        textColor.value = hexColor;
    }

    // Background opacity (extract from rgba)
    const bgOpacitySlider = document.getElementById('overlay-bg-opacity');
    const bgOpacityValue = document.getElementById('overlay-bg-opacity-value');
    if (bgOpacitySlider) {
        const bgColor = config.backgroundColor || defaults.backgroundColor;
        const match = bgColor.match(/rgba?\([^,]+,[^,]+,[^,]+,?\s*([\d.]+)?\)/);
        const opacity = match && match[1] ? Math.round(parseFloat(match[1]) * 100) : 75;
        bgOpacitySlider.value = opacity;
        if (bgOpacityValue) bgOpacityValue.textContent = `${opacity}%`;
    }

    // Show diagnostics
    const showDiagnostics = document.getElementById('toggle-show-diagnostics');
    if (showDiagnostics) showDiagnostics.checked = config.showDiagnostics === true;
}

function applyOcrSettings(translation) {
    const config = normalizeOcrConfig(translation?.ocr);
    
    // Confidence threshold
    const confidenceSlider = document.getElementById('ocr-confidence');
    const confidenceValue = document.getElementById('ocr-confidence-value');
    if (confidenceSlider) {
        const confVal = config.confidenceThreshold * 100;
        confidenceSlider.value = confVal;
        if (confidenceValue) confidenceValue.textContent = config.confidenceThreshold.toFixed(2);
    }
    
    // Validation strictness
    const strictnessSelect = document.getElementById('ocr-strictness');
    if (strictnessSelect) {
        strictnessSelect.value = config.validationStrictness || 'moderate';
    }
    
    // Image preprocessing
    const preprocessingToggle = document.getElementById('toggle-ocr-preprocessing');
    if (preprocessingToggle) preprocessingToggle.checked = config.preprocessingEnabled;
    
    // Grayscale
    const grayscaleToggle = document.getElementById('toggle-ocr-grayscale');
    if (grayscaleToggle) grayscaleToggle.checked = config.grayscale;
    
    // Contrast enhancement
    const contrastToggle = document.getElementById('toggle-ocr-contrast');
    if (contrastToggle) contrastToggle.checked = config.contrastEnhancement;
    
    // Multi-pass OCR
    const multiPassToggle = document.getElementById('toggle-ocr-multi-pass');
    if (multiPassToggle) multiPassToggle.checked = config.enableMultiPass;
    
    // Pass count
    const passCountGroup = document.getElementById('ocr-pass-count-group');
    const passCountInput = document.getElementById('ocr-pass-count');
    if (passCountGroup && passCountInput) {
        passCountGroup.style.display = config.enableMultiPass ? 'block' : 'none';
        passCountInput.value = config.multiPassCount;
    }
}

function normalizeOcrConfig(ocr) {
    const defaultConfig = {
        confidenceThreshold: 0.5,
        preprocessingEnabled: true,
        grayscale: true,
        contrastEnhancement: true,
        enableMultiPass: false,
        multiPassCount: 2,
        validationStrictness: 'moderate',
    };
    
    if (!ocr) {
        return defaultConfig;
    }
    
    return {
        confidenceThreshold: typeof ocr.confidenceThreshold === 'number' 
            ? Math.max(0, Math.min(1, ocr.confidenceThreshold)) 
            : defaultConfig.confidenceThreshold,
        preprocessingEnabled: ocr.preprocessingEnabled ?? defaultConfig.preprocessingEnabled,
        grayscale: ocr.grayscale ?? defaultConfig.grayscale,
        contrastEnhancement: ocr.contrastEnhancement ?? defaultConfig.contrastEnhancement,
        enableMultiPass: ocr.enableMultiPass ?? defaultConfig.enableMultiPass,
        multiPassCount: typeof ocr.multiPassCount === 'number'
            ? Math.max(1, Math.min(5, ocr.multiPassCount))
            : defaultConfig.multiPassCount,
        validationStrictness: ocr.validationStrictness || defaultConfig.validationStrictness,
    };
}

function collectOcrSettings() {
    const confidenceSlider = document.getElementById('ocr-confidence');
    const confidenceValue = confidenceSlider ? parseInt(confidenceSlider.value) / 100 : 0.5;
    
    // Get validation strictness (dropdown)
    const strictnessSelect = document.getElementById('ocr-strictness');
    const validationStrictness = strictnessSelect ? strictnessSelect.value : 'moderate';
    
    return {
        confidenceThreshold: confidenceValue,
        preprocessingEnabled: document.getElementById('toggle-ocr-preprocessing')?.checked ?? true,
        grayscale: document.getElementById('toggle-ocr-grayscale')?.checked ?? true,
        contrastEnhancement: document.getElementById('toggle-ocr-contrast')?.checked ?? true,
        enableMultiPass: document.getElementById('toggle-ocr-multi-pass')?.checked ?? false,
        multiPassCount: Math.max(1, Math.min(5, parseInt(document.getElementById('ocr-pass-count')?.value || '2'))),
        validationStrictness: validationStrictness,
    };
}

function collectOverlaySettings() {
    const fontSize = parseInt(document.getElementById('overlay-font-size')?.value) || 24;
    const fontFamily = document.getElementById('overlay-font-family')?.value || 'Segoe UI';
    const textColor = document.getElementById('overlay-text-color')?.value || '#FFFFFF';
    const bgOpacity = parseInt(document.getElementById('overlay-bg-opacity')?.value) || 75;
    const backgroundColor = `rgba(0, 0, 0, ${bgOpacity / 100})`;
    const showDiagnostics = document.getElementById('toggle-show-diagnostics')?.checked === true;

    return {
        fontSize,
        fontFamily,
        textColor,
        backgroundColor,
        offsetY: 10,
        maxWidth: 0,
        showDiagnostics,
    };
}

async function notifyOverlaySettingsChanged() {
    if (!TauriBridge.event?.emit) return;

    try {
        const overlaySettings = collectOverlaySettings();
        await TauriBridge.event.emit('overlay-settings-updated', overlaySettings);
        console.log('📡 Emitted overlay settings to overlay window:', overlaySettings);
    } catch (e) {
        console.warn('Failed to notify overlay of settings change:', e);
    }
}

function normalizeTranslationConfig(translation) {
    const defaultConfig = {
        enableFoundryLocal: true,
        enableWindowsAi: true,
        enableOfflineMt: true,
        allowMockFallback: true,
        enableContextAware: true,
        contextLevel: 'memoryAndRecent',
        contextRecentCount: 3,
        contextBudgetPercent: 15,
        contextSummaryCooldownMs: 5000,
        promptMaxSourceChars: 300,
        promptMaxContextChars: 600,
        contextBufferSize: 12,
        contextResetGapMs: 6000,
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

    const enableContextAware = translation.enableContextAware ?? defaultConfig.enableContextAware;
    let contextLevel = translation.contextLevel ?? defaultConfig.contextLevel;
    if (!enableContextAware) {
        contextLevel = 'off';
    } else if (contextLevel === 'off') {
        contextLevel = defaultConfig.contextLevel;
    }

    return {
        enableFoundryLocal: translation.enableFoundryLocal ?? defaultConfig.enableFoundryLocal,
        enableWindowsAi: translation.enableWindowsAi ?? defaultConfig.enableWindowsAi,
        enableOfflineMt: translation.enableOfflineMt ?? defaultConfig.enableOfflineMt,
        allowMockFallback: translation.allowMockFallback ?? defaultConfig.allowMockFallback,
        enableContextAware,
        contextLevel,
        contextRecentCount: translation.contextRecentCount ?? defaultConfig.contextRecentCount,
        contextBudgetPercent: translation.contextBudgetPercent ?? defaultConfig.contextBudgetPercent,
        contextSummaryCooldownMs: translation.contextSummaryCooldownMs ?? defaultConfig.contextSummaryCooldownMs,
        promptMaxSourceChars: translation.promptMaxSourceChars ?? defaultConfig.promptMaxSourceChars,
        promptMaxContextChars: translation.promptMaxContextChars ?? defaultConfig.promptMaxContextChars,
        contextBufferSize: translation.contextBufferSize ?? defaultConfig.contextBufferSize,
        contextResetGapMs: translation.contextResetGapMs ?? defaultConfig.contextResetGapMs,
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

function syncContextControls() {
    const level = document.getElementById('context-level')?.value;
    const disabled = level === 'off';
    const recent = document.getElementById('context-recent-count');
    const budget = document.getElementById('context-budget-percent');
    const cooldown = document.getElementById('context-summary-cooldown-ms');
    const maxContext = document.getElementById('prompt-max-context-chars');
    const bufferSize = document.getElementById('context-buffer-size');
    const resetGap = document.getElementById('context-reset-gap-ms');
    if (recent) {
        recent.disabled = level !== 'memoryAndRecent';
    }
    if (budget) {
        budget.disabled = disabled;
    }
    if (cooldown) {
        cooldown.disabled = disabled;
    }
    if (maxContext) {
        maxContext.disabled = disabled;
    }
    if (bufferSize) {
        bufferSize.disabled = disabled;
    }
    if (resetGap) {
        resetGap.disabled = disabled;
    }
}

function clampInt(value, min, max, fallback) {
    if (!Number.isFinite(value)) {
        return fallback;
    }
    return Math.min(Math.max(value, min), max);
}

/**
 * Save settings to Rust backend
 */
async function saveSettings(opts) {
    const options = opts && typeof opts === 'object' ? opts : {};
    const silent = options.silent === true;
    const isAutoSave = options.isAutoSave === true;
    const refreshDiagnostics = options.refreshDiagnostics !== undefined
        ? options.refreshDiagnostics === true
        : !silent;
    const translationConfig = normalizeTranslationConfig(appState.settings?.translation);
    const offlineMtPath = document.getElementById('offline-mt-path').value.trim();
    const foundryModel = document.getElementById('foundry-local-model').value.trim();

    translationConfig.enableFoundryLocal = document.getElementById('toggle-foundry-local').checked;
    translationConfig.enableWindowsAi = document.getElementById('toggle-windows-ai').checked;
    translationConfig.enableOfflineMt = document.getElementById('toggle-offline-mt').checked;
    translationConfig.allowMockFallback = document.getElementById('toggle-mock-fallback').checked;

    const contextLevel = document.getElementById('context-level').value;
    translationConfig.contextLevel = contextLevel;
    translationConfig.enableContextAware = contextLevel !== 'off';
    translationConfig.contextRecentCount = clampInt(
        parseInt(document.getElementById('context-recent-count').value),
        0,
        10,
        3,
    );
    translationConfig.contextBudgetPercent = clampInt(
        parseInt(document.getElementById('context-budget-percent').value),
        5,
        30,
        15,
    );
    translationConfig.contextSummaryCooldownMs = clampInt(
        parseInt(document.getElementById('context-summary-cooldown-ms').value),
        0,
        120000,
        5000,
    );
    translationConfig.promptMaxSourceChars = clampInt(
        parseInt(document.getElementById('prompt-max-source-chars').value),
        50,
        2000,
        300,
    );
    translationConfig.promptMaxContextChars = clampInt(
        parseInt(document.getElementById('prompt-max-context-chars').value),
        0,
        5000,
        600,
    );
    translationConfig.contextBufferSize = clampInt(
        parseInt(document.getElementById('context-buffer-size').value),
        1,
        50,
        12,
    );
    translationConfig.contextResetGapMs = clampInt(
        parseInt(document.getElementById('context-reset-gap-ms').value),
        0,
        120000,
        6000,
    );
    syncContextControls();
    translationConfig.foundryLocal.model = foundryModel.length > 0 ? foundryModel : null;
    translationConfig.offlineMt.binaryPath = offlineMtPath.length > 0 ? offlineMtPath : null;

    // Collect OCR settings
    const ocrConfig = collectOcrSettings();
    
    // Merge OCR config into translation config
    translationConfig.ocr = ocrConfig;
    
    const settings = {
        sourceLanguage: document.getElementById('source-language').value,
        targetLanguage: document.getElementById('target-language').value,
        captureIntervalMs: parseInt(document.getElementById('capture-interval').value),
        overlay: collectOverlaySettings(),
        autoStart: false,
        minimizeToTray: true,
        startWithWindows: false,
        translation: translationConfig,
    };

    try {
        await TauriBridge.invoke('save_settings', { settings });
        appState.settings = settings;
        if (isAutoSave) {
            showToast('✅ Settings saved', 'success');
        } else if (!silent) {
            showToast('Settings saved!', 'success');
        }
        console.log('Settings saved:', settings);
        if (refreshDiagnostics) {
            const refreshTask = refreshTranslationDiagnostics();
            if (!silent) {
                await refreshTask;
            }
        }
    } catch (error) {
        console.error('Failed to save settings:', error);
        if (isAutoSave) {
            showToast('❌ Failed to save settings', 'error');
        } else if (!silent) {
            showToast('Failed to save settings', 'error');
        }
    }
}

const AUTO_SAVE_DELAY_MS = 350;
let autoSaveTimer = null;

function scheduleAutoSave() {
    if (autoSaveTimer) {
        clearTimeout(autoSaveTimer);
    }

    autoSaveTimer = setTimeout(async () => {
        autoSaveTimer = null;
        // Save immediately without silent mode to trigger toast notification
        await saveSettings({ silent: false, refreshDiagnostics: false, isAutoSave: true });
    }, AUTO_SAVE_DELAY_MS);
}

/**
 * Load available Foundry Local models and populate the dropdown
 */
async function loadFoundryLocalModels(selectedModel = null) {
    const select = document.getElementById('foundry-local-model');
    if (!select) return;

    try {
        const desiredModel = selectedModel ?? select.value;
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

        // Ensure the selected model is shown even if it's not in the cached list yet
        if (desiredModel) {
            const hasDesired = Array.from(select.options).some(opt => opt.value === desiredModel);
            if (!hasDesired) {
                const option = document.createElement('option');
                option.value = desiredModel;
                option.textContent = `${desiredModel} (selected)`;
                select.appendChild(option);
            }
            select.value = desiredModel;
        } else {
            select.value = '';
        }

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

    // Save settings button (optional - may not exist with autosave enabled)
    document.getElementById('btn-save-settings')?.addEventListener('click', saveSettings);

    // Capture interval slider
    document.getElementById('capture-interval').addEventListener('input', (e) => {
        document.getElementById('interval-value').textContent = e.target.value;
    });
    document.getElementById('capture-interval').addEventListener('change', () => {
        console.log('Capture interval changed, auto-saving...');
        scheduleAutoSave();
    });

    // OCR settings controls
    const ocrConfidence = document.getElementById('ocr-confidence');
    if (ocrConfidence) {
        ocrConfidence.addEventListener('input', (e) => {
            const value = e.target.value / 100;
            document.getElementById('ocr-confidence-value').textContent = value.toFixed(2);
        });
        ocrConfidence.addEventListener('change', () => {
            console.log('OCR confidence changed, auto-saving...');
            scheduleAutoSave();
        });
    }

    // Multi-pass toggle - show/hide pass count
    const multiPassToggle = document.getElementById('toggle-ocr-multi-pass');
    if (multiPassToggle) {
        multiPassToggle.addEventListener('change', (e) => {
            const passCountGroup = document.getElementById('ocr-pass-count-group');
            if (passCountGroup) {
                passCountGroup.style.display = e.target.checked ? 'block' : 'none';
            }
            scheduleAutoSave();
        });
    }

    // Auto-save on any OCR toggle or dropdown change
    const ocrToggles = ['toggle-ocr-preprocessing', 'toggle-ocr-grayscale', 'toggle-ocr-contrast', 'ocr-pass-count'];
    ocrToggles.forEach(id => {
        const el = document.getElementById(id);
        if (el) {
            el.addEventListener('change', () => {
                console.log('OCR setting changed, auto-saving...');
                scheduleAutoSave();
            });
        }
    });

    // Validation strictness dropdown
    const ocrStrictness = document.getElementById('ocr-strictness');
    if (ocrStrictness) {
        ocrStrictness.addEventListener('change', () => {
            console.log('OCR strictness changed, auto-saving...');
            scheduleAutoSave();
        });
    }

    // Foundry Local controls
    document.getElementById('btn-foundry-refresh')
        .addEventListener('click', handleFoundryRefresh);
    document.getElementById('btn-foundry-make-ready')
        .addEventListener('click', handleFoundryMakeReady);
    document.getElementById('btn-foundry-wizard')
        .addEventListener('click', () => TauriBridge.invoke('open_foundry_wizard'));

    // Listen for wizard completion to auto-configure
    TauriBridge.event.listen('foundry-wizard-closed', async (event) => {
        const result = event.payload;
        if (result?.modelDownloaded) {
            // Auto-enable Foundry and set the downloaded model
            const toggle = document.getElementById('toggle-foundry-local');
            if (toggle) toggle.checked = true;
            if (result.selectedModel) {
                const select = document.getElementById('foundry-local-model');
                if (select) {
                    // Add option if not present
                    let found = false;
                    for (const opt of select.options) {
                        if (opt.value === result.selectedModel) { found = true; break; }
                    }
                    if (!found) {
                        const opt = document.createElement('option');
                        opt.value = result.selectedModel;
                        opt.textContent = result.selectedModel;
                        select.appendChild(opt);
                    }
                    select.value = result.selectedModel;
                }
            }
            await saveSettings({ silent: false, refreshDiagnostics: true });
        }
        await refreshFoundryStatus({ probe: true, reason: 'wizard-closed' });
        await loadFoundryLocalModels();
    });

    // Save model selection immediately (users expect this to persist across restarts)
    document.getElementById('foundry-local-model').addEventListener('change', async () => {
        console.log('Foundry Local model changed, saving...');
        await saveSettings({ silent: true, refreshDiagnostics: false });
        void refreshFoundryStatus({ probe: true, reason: 'model-change' });
    });

    // Auto-save when language settings change to ensure translation direction is persisted
    document.getElementById('source-language').addEventListener('change', () => {
        console.log('Source language changed, auto-saving...');
        checkOcrLanguageWarning(document.getElementById('source-language').value);
        scheduleAutoSave();
    });

    // Install OCR language pack button
    document.getElementById('ocr-lang-install-btn').addEventListener('click', () => {
        installOcrLanguage();
    });
    document.getElementById('target-language').addEventListener('change', () => {
        console.log('Target language changed, auto-saving...');
        scheduleAutoSave();
    });

    // Auto-save when context controls change
    document.getElementById('context-level').addEventListener('change', () => {
        syncContextControls();
        console.log('Context level changed, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('context-recent-count').addEventListener('change', () => {
        console.log('Context recent count changed, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('context-budget-percent').addEventListener('change', () => {
        console.log('Context budget percent changed, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('context-summary-cooldown-ms').addEventListener('change', () => {
        console.log('Context summary cooldown changed, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('prompt-max-source-chars').addEventListener('change', () => {
        console.log('Prompt max source chars changed, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('prompt-max-context-chars').addEventListener('change', () => {
        console.log('Prompt max context chars changed, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('context-buffer-size').addEventListener('change', () => {
        console.log('Context buffer size changed, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('context-reset-gap-ms').addEventListener('change', () => {
        console.log('Context reset gap changed, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('toggle-foundry-local').addEventListener('change', () => {
        console.log('Foundry Local toggled, auto-saving...');
        scheduleAutoSave();
        void refreshFoundryStatus({ probe: false, reason: 'toggle' });
        scheduleFoundryAutoProbe();
    });
    document.getElementById('toggle-windows-ai').addEventListener('change', () => {
        console.log('Windows AI toggled, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('toggle-offline-mt').addEventListener('change', () => {
        console.log('Offline MT toggled, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('toggle-mock-fallback').addEventListener('change', () => {
        console.log('Passthrough fallback toggled, auto-saving...');
        scheduleAutoSave();
    });
    document.getElementById('offline-mt-path').addEventListener('change', () => {
        console.log('Offline MT path changed, auto-saving...');
        scheduleAutoSave();
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

    // Foundry warmup modal actions
    document.getElementById('btn-foundry-warmup-cancel')
        .addEventListener('click', closeFoundryWarmupModal);
    document.getElementById('foundry-warmup-backdrop')
        .addEventListener('click', closeFoundryWarmupModal);
    document.getElementById('btn-foundry-warmup-start')
        .addEventListener('click', handleWarmupFoundryAndStart);
    document.getElementById('btn-foundry-fallback-start')
        .addEventListener('click', handleStartWithFallbackNow);

    // Overlay appearance settings
    const overlayFontSize = document.getElementById('overlay-font-size');
    if (overlayFontSize) {
        overlayFontSize.addEventListener('input', (e) => {
            document.getElementById('overlay-font-size-value').textContent = e.target.value;
        });
        overlayFontSize.addEventListener('change', () => {
            console.log('Overlay font size changed, auto-saving...');
            scheduleAutoSave();
            notifyOverlaySettingsChanged();
        });
    }

    const overlayFontFamily = document.getElementById('overlay-font-family');
    if (overlayFontFamily) {
        overlayFontFamily.addEventListener('change', () => {
            console.log('Overlay font family changed, auto-saving...');
            scheduleAutoSave();
            notifyOverlaySettingsChanged();
        });
    }

    const overlayTextColor = document.getElementById('overlay-text-color');
    if (overlayTextColor) {
        overlayTextColor.addEventListener('change', () => {
            console.log('Overlay text color changed, auto-saving...');
            scheduleAutoSave();
            notifyOverlaySettingsChanged();
        });
    }

    const overlayBgOpacity = document.getElementById('overlay-bg-opacity');
    if (overlayBgOpacity) {
        overlayBgOpacity.addEventListener('input', (e) => {
            const opacityValue = document.getElementById('overlay-bg-opacity-value');
            if (opacityValue) opacityValue.textContent = `${e.target.value}%`;
        });
        overlayBgOpacity.addEventListener('change', () => {
            console.log('Overlay background opacity changed, auto-saving...');
            scheduleAutoSave();
            notifyOverlaySettingsChanged();
        });
    }

    const showDiagnosticsToggle = document.getElementById('toggle-show-diagnostics');
    if (showDiagnosticsToggle) {
        showDiagnosticsToggle.addEventListener('change', () => {
            console.log('Show diagnostics toggled, auto-saving...');
            scheduleAutoSave();
            notifyOverlaySettingsChanged();
        });
    }
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
        const result = await TauriBridge.invoke('open_area_selector');
        console.log('Selector window opened:', result);

        if (result && result.mode === 'legacy') {
            showToast('Premium legacy selector opened', 'success');
        } else if (result && result.mode === 'winui') {
            showToast('WinUI selector opened (experimental)', 'warning');
        }

        // Start polling for region changes (fallback in case events don't work)
        await startRegionPolling();
    } catch (error) {
        console.error('Failed to open area selector:', error);
        showToast('Failed to open selector: ' + error, 'error');
    }
}

/**
 * Poll for region updates as a fallback mechanism
 */
let pollingInterval = null;

async function startRegionPolling() {
    // Stop any existing polling
    if (pollingInterval) {
        clearInterval(pollingInterval);
    }

    // Establish a baseline from the backend so restored regions don't trigger a false "selected" toast
    let baselineRegion = null;
    try {
        baselineRegion = await TauriBridge.invoke('get_capture_region');
    } catch (e) {
        // Ignore baseline fetch errors; we'll fall back to current app state
    }
    baselineRegion = baselineRegion || appState.captureRegion;

    let attempts = 0;
    const maxAttempts = 100; // 10 seconds max

    pollingInterval = setInterval(async () => {
        attempts++;

        try {
            const region = await TauriBridge.invoke('get_capture_region');

            // Check if we got a new region
            if (region && (!baselineRegion ||
                region.x !== baselineRegion.x ||
                region.y !== baselineRegion.y ||
                region.width !== baselineRegion.width ||
                region.height !== baselineRegion.height)) {

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

                // Now that the user intends to translate, warm up Foundry readiness in the background.
                scheduleFoundryAutoProbe();
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

            // Now that the user intends to translate, warm up Foundry readiness in the background.
            scheduleFoundryAutoProbe();
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

    // Stop any background Foundry probing now that translation intent is gone.
    const state = appState.foundryAutoProbe;
    state.attempts = 0;
    if (state.timerId) {
        clearTimeout(state.timerId);
        state.timerId = null;
    }
    void refreshFoundryStatus({ probe: false, reason: 'region-cleared' });
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

        // Keep Foundry card (top priority backend) in sync with config/status changes.
        void refreshFoundryStatus({ probe: false, reason: 'diagnostics' });
    } catch (error) {
        console.error('Failed to load backend diagnostics:', error);
        container.innerHTML = '<div class="backend-status-empty">Failed to load backend status.</div>';
    }
}

function escapeHtml(value) {
    return (value ?? '')
        .toString()
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/\"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function formatAgeMs(ageMs) {
    if (!Number.isFinite(ageMs) || ageMs < 0) {
        return '--';
    }
    if (ageMs < 1000) {
        return 'just now';
    }
    if (ageMs < 60_000) {
        return `${Math.round(ageMs / 1000)}s ago`;
    }
    if (ageMs < 3_600_000) {
        return `${Math.round(ageMs / 60_000)}m ago`;
    }
    return `${Math.round(ageMs / 3_600_000)}h ago`;
}

function formatEpochMsDelta(epochMs) {
    if (!Number.isFinite(epochMs) || epochMs <= 0) {
        return 'never';
    }
    return formatAgeMs(Date.now() - epochMs);
}

function renderFoundryStatus(status) {
    const statusEl = document.getElementById('foundry-status');
    if (!statusEl) {
        return;
    }

    if (!status) {
        statusEl.innerHTML = '<span class="status-text">Foundry Local status unavailable.</span>';
        return;
    }

    appState.foundryStatus.last = status;

    const enabled = document.getElementById('toggle-foundry-local')?.checked === true;
    const phase = (status.phase || '').toString();

    const installedOk = status.cliAvailable === true;
    const serviceOk = status.serviceRunning === true;

    const installedStepClass = installedOk ? 'done' : 'error';
    const serviceStepClass = serviceOk ? 'done' : '';

    let modelStepClass = '';
    if (phase === 'ready') {
        modelStepClass = 'done';
    } else if (phase === 'preparing' || phase === 'unchecked') {
        modelStepClass = 'active';
    } else if (phase === 'error') {
        modelStepClass = 'error';
    }

    const installedPill = installedOk
        ? '<span class="status-pill ready">● OK</span>'
        : '<span class="status-pill error">● MISSING</span>';

    const servicePill = serviceOk
        ? '<span class="status-pill ready">● RUNNING</span>'
        : '<span class="status-pill not-ready">● STOPPED</span>';

    const modelInfo = formatFoundryPhase(phase);
    const modelPill = `<span class="status-pill ${modelInfo.className}">● ${modelInfo.label.toUpperCase()}</span>`;

    const serviceDesc = serviceOk
        ? `Service URL: ${escapeHtml(status.serviceUrl || '')}`
        : installedOk
            ? 'Service not running. Click "Make Foundry Ready" to start it.'
            : 'Install Foundry Local first (recommended: winget install Microsoft.FoundryLocal).';

    const modelDesc = escapeHtml(status.notes || '');

    const configuredRaw = status.configuredModel || '';
    const resolvedRaw = status.selectedModel || '';

    let modelDisplay = '(auto)';
    if (!configuredRaw && resolvedRaw) {
        modelDisplay = `(auto) \u2192 ${escapeHtml(resolvedRaw)}`;
    } else if (configuredRaw && resolvedRaw && configuredRaw !== resolvedRaw) {
        modelDisplay = `${escapeHtml(configuredRaw)} \u2192 ${escapeHtml(resolvedRaw)}`;
    } else if (configuredRaw) {
        modelDisplay = escapeHtml(configuredRaw);
    } else if (resolvedRaw) {
        modelDisplay = escapeHtml(resolvedRaw);
    }

    const probeAttemptMs = status.probe?.lastAttemptMs;
    const checkedEpochMs = Number.isFinite(probeAttemptMs)
        ? probeAttemptMs
        : appState.foundryStatus.lastCheckedMs;
    const lastChecked = formatEpochMsDelta(checkedEpochMs);

    let lastError = '';
    if (status.probe?.result === 'error') {
        lastError = status.probe?.error || 'Unknown error';
    } else if (status.probe?.result === 'timeout') {
        lastError = 'Probe timed out (model likely warming up).';
    }

    const lastErrorRow = lastError
        ? `
            <div class="foundry-meta-row error">
                <span class="foundry-meta-label">Last error</span>
                <span class="foundry-meta-value" id="foundry-last-error">${escapeHtml(lastError)}</span>
            </div>
        `
        : '';

    const disabledNote = enabled
        ? ''
        : '<div class="foundry-step-desc">Foundry Local is currently disabled (toggle above).</div>';

    const installBanner = !installedOk
        ? `<div class="foundry-install-banner">
            <strong>Foundry Local is not installed.</strong>
            Click <em>Make Foundry Ready</em> to launch the setup wizard, or use the <em>Setup Wizard</em> button below.
        </div>`
        : '';

    statusEl.innerHTML = `
        ${installBanner}
        <div class="foundry-steps">
            <div class="foundry-step ${installedStepClass}">
                <div class="foundry-step-dot"></div>
                <div class="foundry-step-body">
                    <div class="foundry-step-title">
                        <span>Installed</span>
                        ${installedPill}
                    </div>
                    <div class="foundry-step-desc">${installedOk ? 'Foundry CLI detected.' : 'Foundry CLI not found.'}</div>
                </div>
            </div>
            <div class="foundry-step ${serviceStepClass}">
                <div class="foundry-step-dot"></div>
                <div class="foundry-step-body">
                    <div class="foundry-step-title">
                        <span>Service running</span>
                        ${servicePill}
                    </div>
                    <div class="foundry-step-desc">${serviceDesc}</div>
                </div>
            </div>
            <div class="foundry-step ${modelStepClass}">
                <div class="foundry-step-dot"></div>
                <div class="foundry-step-body">
                    <div class="foundry-step-title">
                        <span>Model ready (probe)</span>
                        ${modelPill}
                    </div>
                    <div class="foundry-step-desc">${modelDesc}</div>
                    ${disabledNote}
                </div>
            </div>
        </div>
        <div class="foundry-meta">
            <div class="foundry-meta-row">
                <span class="foundry-meta-label">Model</span>
                <span class="foundry-meta-value">${modelDisplay}</span>
            </div>
            <div class="foundry-meta-row">
                <span class="foundry-meta-label">Last checked</span>
                <span class="foundry-meta-value" id="foundry-last-checked">${escapeHtml(lastChecked)}</span>
            </div>
            ${lastErrorRow}
        </div>
    `;
}

function renderFoundryStatusChecking(message) {
    const statusEl = document.getElementById('foundry-status');
    if (!statusEl) {
        return;
    }

    const text = message || 'Checking Foundry Local...';
    statusEl.innerHTML = `
        <div class="foundry-steps">
            <div class="foundry-step active">
                <div class="foundry-step-dot"></div>
                <div class="foundry-step-body">
                    <div class="foundry-step-title">
                        <span>Checking</span>
                        <span class="status-pill checking">● CHECKING</span>
                    </div>
                    <div class="foundry-step-desc">${escapeHtml(text)}</div>
                </div>
            </div>
        </div>
    `;
}

function renderFoundryStatusError(error) {
    const statusEl = document.getElementById('foundry-status');
    if (!statusEl) {
        return;
    }

    const message = error?.message ? error.message : String(error);
    statusEl.innerHTML = `
        <div class="foundry-steps">
            <div class="foundry-step error">
                <div class="foundry-step-dot"></div>
                <div class="foundry-step-body">
                    <div class="foundry-step-title">
                        <span>Foundry Local</span>
                        <span class="status-pill error">● ERROR</span>
                    </div>
                    <div class="foundry-step-desc">${escapeHtml(message)}</div>
                </div>
            </div>
        </div>
    `;
}

async function refreshFoundryStatus(opts) {
    const options = opts && typeof opts === 'object' ? opts : {};
    const probe = options.probe === true;
    const reason = (options.reason || '').toString();

    const enabled = document.getElementById('toggle-foundry-local')?.checked === true;
    if (probe && !enabled && reason === 'auto') {
        return null;
    }

    const state = appState.foundryAutoProbe;
    if (state.inFlight) {
        return appState.foundryStatus.last;
    }

    state.inFlight = true;
    try {
        const shouldRenderChecking = probe && (reason !== 'auto' || !appState.foundryStatus.last);
        if (shouldRenderChecking) {
            renderFoundryStatusChecking('Verifying model readiness...');
        }

        const status = probe
            ? await TauriBridge.invoke('refresh_foundry_local_status')
            : await TauriBridge.invoke('get_foundry_local_status');

        appState.foundryStatus.lastCheckedMs = Date.now();
        renderFoundryStatus(status);
        maybeAutoProbeFoundry(status);
        return status;
    } catch (e) {
        console.warn('Foundry Local status refresh failed:', e);
        renderFoundryStatusError(e);
        return null;
    } finally {
        state.inFlight = false;
    }
}

async function handleFoundryRefresh() {
    const button = document.getElementById('btn-foundry-refresh');
    if (!button) {
        return;
    }

    button.disabled = true;
    const originalLabel = button.textContent;
    button.textContent = 'Refreshing...';

    try {
        // Ensure the backend uses the currently selected model before probing.
        await saveSettings({ silent: true, refreshDiagnostics: false });
        const status = await refreshFoundryStatus({ probe: true, reason: 'manual' });
        void refreshTranslationDiagnostics();
        if (status?.phase === 'ready') {
            showToast('Foundry Local ready!', 'success');
        }
    } catch (e) {
        const message = e?.message ? e.message : String(e);
        showToast(`Foundry refresh failed: ${message}`, 'error');
    } finally {
        button.disabled = false;
        button.textContent = originalLabel;
    }
}

async function handleFoundryMakeReady() {
    const button = document.getElementById('btn-foundry-make-ready');
    if (!button) {
        return;
    }

    button.disabled = true;
    const originalLabel = button.textContent;
    button.textContent = 'Checking...';

    try {
        // Quick status check: if CLI not installed, open the wizard instead
        await saveSettings({ silent: true, refreshDiagnostics: false });
        const quickStatus = await TauriBridge.invoke('get_foundry_local_status');
        if (!quickStatus.cliAvailable) {
            button.disabled = false;
            button.textContent = originalLabel;
            await TauriBridge.invoke('open_foundry_wizard');
            return;
        }
    } catch (e) {
        // If quick check fails, fall through to normal flow
        console.warn('Quick status check failed, continuing with make_foundry_ready:', e);
    }

    button.textContent = 'Making ready...';
    renderFoundryStatusChecking('Starting service (if needed) and warming up model...');

    try {
        // Ensure the backend uses the currently selected model before warmup.
        await saveSettings({ silent: true, refreshDiagnostics: false });
        const status = await TauriBridge.invoke('make_foundry_ready');
        renderFoundryStatus(status);
        void refreshTranslationDiagnostics();

        if (status?.phase === 'ready') {
            showToast('Foundry Local ready!', 'success');
        } else if (status?.notes) {
            showToast(status.notes, 'warning');
        }

        // The service may now expose more models; refresh the dropdown.
        await loadFoundryLocalModels(status?.selectedModel || null);
    } catch (e) {
        console.error('Failed to make Foundry ready:', e);
        const message = e?.message ? e.message : String(e);
        showToast(`Make Foundry Ready failed: ${message}`, 'error');
        renderFoundryStatusError(e);
    } finally {
        button.disabled = false;
        button.textContent = originalLabel;
    }
}

function scheduleFoundryAutoProbe() {
    // Don't block startup; run shortly after initial diagnostics paint.
    setTimeout(() => {
        const enabled = document.getElementById('toggle-foundry-local')?.checked === true;
        const shouldProbe = enabled && appState.isRunning !== true;
        void refreshFoundryStatus({ probe: shouldProbe, reason: 'auto' });
    }, 300);
}

function maybeAutoProbeFoundry(status) {
    // Keep auto-probing light: do one fast check on startup, and at most a couple of
    // follow-ups if Foundry is still warming up.
    if (appState.isRunning === true) {
        const state = appState.foundryAutoProbe;
        state.attempts = 0;
        if (state.timerId) {
            clearTimeout(state.timerId);
            state.timerId = null;
        }
        return;
    }

    const enabled = document.getElementById('toggle-foundry-local')?.checked === true;
    if (!enabled) {
        const state = appState.foundryAutoProbe;
        state.attempts = 0;
        if (state.timerId) {
            clearTimeout(state.timerId);
            state.timerId = null;
        }
        return;
    }

    if (!status || status.serviceRunning !== true || !Array.isArray(status.models) || status.models.length === 0) {
        return;
    }

    const phase = (status.phase || '').toString();
    if (phase === 'ready') {
        const state = appState.foundryAutoProbe;
        state.attempts = 0;
        if (state.timerId) {
            clearTimeout(state.timerId);
            state.timerId = null;
        }
        return;
    }

    const state = appState.foundryAutoProbe;
    if (state.attempts >= 3) {
        return;
    }

    let delayMs = null;
    if (phase === 'unchecked') {
        delayMs = 800;
    } else if (phase === 'preparing') {
        delayMs = 25_000;
    } else if (phase === 'error') {
        delayMs = 15_000;
    } else {
        return;
    }

    if (state.timerId) {
        clearTimeout(state.timerId);
    }
    state.timerId = setTimeout(() => {
        void refreshFoundryStatus({ probe: true, reason: 'auto' });
    }, delayMs);
    state.attempts += 1;
    state.lastAttemptMs = Date.now();
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

/**
 * Format Foundry Local phase into display info
 * @param {string} phase - The phase from backend: notInstalled, notRunning, noModels, preparing, ready, error
 * @returns {{ label: string, className: string }}
 */
function formatFoundryPhase(phase) {
    switch (phase) {
        case 'ready':
            return { label: 'Ready', className: 'ready' };
        case 'unchecked':
            return { label: 'Not checked', className: 'unchecked' };
        case 'preparing':
            return { label: 'Preparing', className: 'preparing' };
        case 'notRunning':
        case 'notrunning':
            return { label: 'Not Running', className: 'not-ready' };
        case 'noModels':
        case 'nomodels':
            return { label: 'No Models', className: 'not-ready' };
        case 'notInstalled':
        case 'notinstalled':
            return { label: 'Not Installed', className: 'not-supported' };
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

        let notesText = backend.notes || '';
        if (extra) {
            notesText = notesText ? `${notesText} ${extra}` : extra;
        }
        notes.textContent = notesText || 'No notes available.';

        row.appendChild(header);
        row.appendChild(notes);
        container.appendChild(row);
    });

    // Update inline status cards
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

    // Prefer phase field if available (more granular), fall back to readyState
    let statusInfo;
    if (foundry.phase) {
        statusInfo = formatFoundryPhase(foundry.phase);
    } else {
        let readyState = foundry.readyState;
        if (!foundry.available && readyState === 'ready') {
            readyState = 'notReady';
        }
        statusInfo = formatReadyState(readyState);
    }

    const statusClass = statusInfo.className;
    const pill = `<span class="status-pill ${statusClass}">● ${statusInfo.label.toUpperCase()}</span>`;

    const backendKey = backendIdKey(foundry.id);
    const errorCode = diagnostics.lastErrorByBackend?.[backendKey];
    const latency = diagnostics.lastLatencyByBackend?.[backendKey];
    let extra = '';
    if (typeof latency === 'number') {
        extra = `Last latency: ${latency}ms.`;
    }
    if (errorCode) {
        extra = extra ? `${extra} Last error: ${errorCode}.` : `Last error: ${errorCode}.`;
    }

    let notesText = foundry.notes || '';
    if (extra) {
        notesText = notesText ? `${notesText} ${extra}` : extra;
    }

    statusEl.innerHTML = `
        ${pill}
        <span class="status-text">${notesText}</span>
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

function openFoundryWarmupModal(status) {
    const modal = document.getElementById('foundry-warmup-modal');
    const body = document.getElementById('foundry-warmup-status');
    if (!modal || !body) {
        return;
    }

    const phase = (status?.phase || 'error').toString();
    const phaseInfo = formatFoundryPhase(phase);

    const configuredRaw = status?.configuredModel || '';
    const resolvedRaw = status?.selectedModel || '';
    let modelDisplay = '(auto)';
    if (!configuredRaw && resolvedRaw) {
        modelDisplay = `(auto) \u2192 ${resolvedRaw}`;
    } else if (configuredRaw && resolvedRaw && configuredRaw !== resolvedRaw) {
        modelDisplay = `${configuredRaw} \u2192 ${resolvedRaw}`;
    } else if (configuredRaw) {
        modelDisplay = configuredRaw;
    } else if (resolvedRaw) {
        modelDisplay = resolvedRaw;
    }
    const lastError =
        status?.probe?.result === 'error'
            ? (status.probe.error || 'Unknown error')
            : status?.probe?.result === 'timeout'
                ? 'Probe timed out (model likely warming up).'
                : '';

    const installHint = !status?.cliAvailable
        ? 'Install Foundry Local: winget install Microsoft.FoundryLocal'
        : '';

    const serviceHint = status?.cliAvailable && status?.serviceRunning !== true
        ? 'Start the service with "Make Foundry Ready" (recommended) or: foundry service start'
        : '';

    const notes = status?.notes ? status.notes : 'Could not determine Foundry Local status.';

    body.innerHTML = `
        <div style="margin-bottom: 10px;">
            Foundry Local is enabled, but the model is not ready yet.
        </div>
        <div style="margin-bottom: 10px;">
            <span class="status-pill ${escapeHtml(phaseInfo.className)}">● ${escapeHtml(phaseInfo.label.toUpperCase())}</span>
        </div>
        <div class="setting-hint" style="margin-bottom: 10px;">
            Model: ${escapeHtml(modelDisplay)}
        </div>
        <div class="setting-hint" style="margin-bottom: 10px;">
            ${escapeHtml(notes)}
        </div>
        ${lastError ? `<div class="setting-hint" style="color:#f44336;">Last error: ${escapeHtml(lastError)}</div>` : ''}
        ${installHint ? `<div class="setting-hint" style="margin-top:10px;">${escapeHtml(installHint)}</div>` : ''}
        ${serviceHint ? `<div class="setting-hint" style="margin-top:6px;">${escapeHtml(serviceHint)}</div>` : ''}
    `;

    // Reset modal buttons in case a previous warmup attempt disabled them.
    const warmup = document.getElementById('btn-foundry-warmup-start');
    const fallback = document.getElementById('btn-foundry-fallback-start');
    const cancel = document.getElementById('btn-foundry-warmup-cancel');
    if (warmup) warmup.disabled = false;
    if (fallback) fallback.disabled = false;
    if (cancel) cancel.disabled = false;

    modal.classList.remove('hidden');
}

function closeFoundryWarmupModal() {
    const modal = document.getElementById('foundry-warmup-modal');
    if (modal) {
        modal.classList.add('hidden');
    }
}

async function handleWarmupFoundryAndStart() {
    const warmup = document.getElementById('btn-foundry-warmup-start');
    const fallback = document.getElementById('btn-foundry-fallback-start');
    const cancel = document.getElementById('btn-foundry-warmup-cancel');
    const body = document.getElementById('foundry-warmup-status');

    if (warmup) warmup.disabled = true;
    if (fallback) fallback.disabled = true;
    if (cancel) cancel.disabled = true;
    if (body) {
        body.textContent = 'Warming up Foundry Local... (this can take a bit on first run)';
    }

    try {
        await saveSettings({ silent: true, refreshDiagnostics: false });
        const status = await TauriBridge.invoke('make_foundry_ready');
        renderFoundryStatus(status);

        if (status?.phase === 'ready') {
            closeFoundryWarmupModal();
            await startTranslationNow();
            return;
        }

        // Still not ready - keep the modal open and show details + allow fallback.
        openFoundryWarmupModal(status);
    } catch (e) {
        console.error('Foundry warmup failed:', e);
        if (body) {
            const message = e?.message ? e.message : String(e);
            body.textContent = `Warmup failed: ${message}`;
        }
        if (fallback) fallback.disabled = false;
        if (cancel) cancel.disabled = false;
        if (warmup) warmup.disabled = false;
    }
}

async function handleStartWithFallbackNow() {
    closeFoundryWarmupModal();
    showToast('Starting with fallback (Foundry not ready).', 'warning');
    await saveSettings({ silent: true, refreshDiagnostics: false });
    await startTranslationNow();
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
 * Start translation (actual backend start)
 */
async function startTranslationNow() {
    const startButton = document.getElementById('btn-start');
    const stopButton = document.getElementById('btn-stop');

    // Provide immediate UI feedback (backend startup can take a moment)
    if (startButton) {
        startButton.disabled = true;
    }
    updateStatus('running', 'Starting...');
    showToast('Starting translation...', 'success');

    try {
        await TauriBridge.invoke('start_translation');
        appState.isRunning = true;

        // Reconcile UI with backend truth (helps if the backend returned early or got stuck).
        await syncTranslationState();
        showToast('Translation started!', 'success');
    } catch (error) {
        console.error('Failed to start translation:', error);
        appState.isRunning = false;

        // Restore UI state on failure
        if (stopButton) {
            stopButton.style.display = 'none';
        }
        if (startButton) {
            startButton.style.display = 'flex';
            startButton.disabled = !appState.captureRegion;
        }
        updateStatus('ready', 'Ready');
        showToast('Failed to start: ' + error, 'error');

        // If the backend got stuck in a running state, sync will restore the Stop button.
        try {
            await syncTranslationState();
        } catch (e) {
            // Ignore sync errors.
        }
    }
}

/**
 * Start translation (UX gate: if Foundry is enabled but not Ready, prompt user)
 */
async function handleStartTranslation() {
    console.log('Start translation clicked');

    // Ensure backend is using the current UI settings (languages, model selection, toggles).
    await saveSettings({ silent: true, refreshDiagnostics: false });

    const foundryEnabled = document.getElementById('toggle-foundry-local')?.checked === true;
    if (foundryEnabled) {
        const status = await refreshFoundryStatus({ probe: true, reason: 'start-translation' });
        if (!status || status.phase !== 'ready') {
            openFoundryWarmupModal(status);
            return;
        }
    }

    await startTranslationNow();
}

/**
 * Stop translation
 */
async function handleStopTranslation() {
    console.log('Stopping translation...');

    const startButton = document.getElementById('btn-start');
    const stopButton = document.getElementById('btn-stop');

    try {
        await TauriBridge.invoke('stop_translation');
        appState.isRunning = false;

        // Update UI
        if (stopButton) {
            stopButton.style.display = 'none';
        }
        if (startButton) {
            startButton.style.display = 'flex';
            startButton.disabled = !appState.captureRegion;
        }
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
