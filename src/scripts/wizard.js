// =============================================================================
// WIZARD.JS - Local Translation Engine Setup
// =============================================================================
// Controls the 4-step wizard flow:
//   1. Explain the private local engine
//   2. Configure cache directory
//   3. Download a model
//   4. Verify & done
// =============================================================================

(function () {
    'use strict';

    // =========================================================================
    // STATE
    // =========================================================================

    const state = {
        currentStep: 1,
        selectedModel: null,
        modelDownloaded: false,
        downloadInProgress: false,
    };

    // =========================================================================
    // DOM HELPERS
    // =========================================================================

    function $(id) { return document.getElementById(id); }

    function show(el) {
        if (typeof el === 'string') el = $(el);
        if (el) el.style.display = '';
    }

    function hide(el) {
        if (typeof el === 'string') el = $(el);
        if (el) el.style.display = 'none';
    }

    // =========================================================================
    // STEP INDICATOR
    // =========================================================================

    function updateStepIndicator() {
        const items = document.querySelectorAll('.wizard-step-item');
        const connectors = document.querySelectorAll('.wizard-step-connector');

        items.forEach((item, i) => {
            const stepNum = i + 1;
            item.classList.remove('active', 'done');
            if (stepNum < state.currentStep) {
                item.classList.add('done');
            } else if (stepNum === state.currentStep) {
                item.classList.add('active');
            }
        });

        connectors.forEach((conn, i) => {
            conn.classList.toggle('done', i + 1 < state.currentStep);
        });
    }

    function showStep(step) {
        state.currentStep = step;
        updateStepIndicator();

        // Hide all panels
        document.querySelectorAll('.wizard-panel').forEach(p => p.style.display = 'none');

        // Show the current panel
        const panels = ['step-install', 'step-configure', 'step-model', 'step-verify'];
        show(panels[step - 1]);

        // Update buttons for this step
        updateButtons();

        // Initialize the step
        if (step === 1) initStepInstall();
        if (step === 2) initStepConfigure();
        if (step === 3) initStepModel();
        if (step === 4) initStepVerify();
    }

    // =========================================================================
    // BUTTON MANAGEMENT
    // =========================================================================

    function updateButtons() {
        const btnNext = $('btn-wizard-next');
        const btnSkip = $('btn-wizard-skip');
        const btnRetry = $('btn-wizard-retry');
        const btnCancel = $('btn-wizard-cancel');

        // Reset all optional buttons
        hide(btnSkip);
        hide(btnRetry);
        btnNext.disabled = false;

        switch (state.currentStep) {
            case 1:
                btnNext.textContent = 'Next';
                break;
            case 2:
                btnNext.textContent = 'Next';
                show(btnSkip);
                $('btn-wizard-skip').textContent = 'Use Default';
                break;
            case 3:
                btnNext.textContent = 'Download';
                if (!state.selectedModel) {
                    btnNext.disabled = true;
                }
                break;
            case 4:
                btnNext.textContent = 'Close';
                btnCancel.style.display = 'none';
                break;
        }
    }

    // =========================================================================
    // STEP 1: EXPLAIN THE CURATED LOCAL ENGINE
    // =========================================================================

    async function initStepInstall() {
        show('install-already-installed');
        updateButtons();
    }

    // =========================================================================
    // STEP 2: CONFIGURE CACHE DIRECTORY
    // =========================================================================

    async function initStepConfigure() {
        // Show default cache path
        const defaultPath = await getDefaultCachePath();
        $('cache-dir-path').value = defaultPath;

        // Check disk space
        try {
            const space = await TauriBridge.invoke('wizard_get_disk_space', { path: defaultPath });
            $('disk-space-value').textContent = space.availableDisplay;
            if (space.availableBytes < 5 * 1024 * 1024 * 1024) { // < 5 GB
                $('disk-space-value').classList.add('warning');
            }
        } catch (e) {
            $('disk-space-value').textContent = 'Unknown';
            console.error('Failed to check disk space:', e);
        }
    }

    async function getDefaultCachePath() {
        // Try to get the real user home directory from Tauri path API
        try {
            if (window.__TAURI__ && window.__TAURI__.path) {
                let homeDir = await window.__TAURI__.path.homeDir();
                // Ensure trailing separator before appending
                if (!homeDir.endsWith('\\') && !homeDir.endsWith('/')) {
                    homeDir += '\\';
                }
                return homeDir + '.meowcal-cache';
            }
        } catch {
            // Fall through to the environment placeholder.
        }
        // Fallback: show generic placeholder (actual path depends on user profile)
        return '%USERPROFILE%\\.meowcal-cache';
    }

    async function handleBrowseCache() {
        // Use Tauri dialog plugin for folder selection
        if (window.__TAURI__ && window.__TAURI__.dialog) {
            try {
                const selected = await window.__TAURI__.dialog.open({
                    directory: true,
                    multiple: false,
                    title: 'Select Model Cache Directory',
                });
                if (selected) {
                    $('cache-dir-path').value = selected;
                    // Re-check disk space for the new path
                    try {
                        const space = await TauriBridge.invoke('wizard_get_disk_space', { path: selected });
                        $('disk-space-value').textContent = space.availableDisplay;
                        $('disk-space-value').classList.toggle('warning', space.availableBytes < 5 * 1024 * 1024 * 1024);
                    } catch {
                        $('disk-space-value').textContent = 'Unknown';
                    }
                }
            } catch (e) {
                console.error('Folder picker error:', e);
            }
        }
    }

    // =========================================================================
    // STEP 3: DOWNLOAD MODEL
    // =========================================================================

    async function initStepModel() {
        hide('download-progress');
        hide('wizard-terminal');
        hide('model-disk-warning');
        state.selectedModel = null;
        state.downloadInProgress = false;

        // Show hardware info
        try {
            const hw = await TauriBridge.invoke('wizard_get_hardware_info');
            const badgeEl = $('hardware-info');
            const textEl = $('hardware-badge-text');

            if (hw.hasNpu) {
                textEl.textContent = 'NPU Detected (Snapdragon X) - NPU-optimized models recommended';
            } else if (hw.hasGpu && hw.gpuName) {
                textEl.textContent = `GPU Detected: ${hw.gpuName}`;
            } else if (hw.isArm64) {
                textEl.textContent = 'ARM64 CPU detected';
            } else {
                textEl.textContent = `${hw.arch.toUpperCase()} CPU`;
            }
            show(badgeEl);
        } catch (e) {
            console.error('Failed to get hardware info:', e);
        }

        // Load model list
        const listEl = $('model-list');
        listEl.innerHTML = '<div class="wizard-progress-box"><div class="wizard-spinner"></div><span>Loading available models...</span></div>';

        try {
            const models = await TauriBridge.invoke('wizard_list_available_models');

            if (!models || models.length === 0) {
                listEl.innerHTML = `
                    <div class="wizard-error-box">
                        <strong>The supported translation engine is unavailable.</strong>
                        <p>Close setup, check for an app update, then try Install / Repair again.</p>
                    </div>
                `;
                $('btn-wizard-next').disabled = true;
                return;
            }

            // Render model cards
            listEl.innerHTML = '';
            models.forEach(model => {
                const card = document.createElement('div');
                card.className = 'wizard-model-card';
                if (model.recommended) {
                    card.classList.add('selected');
                    state.selectedModel = model.id;
                }

                let badges = '';
                if (model.recommended) {
                    badges += '<span class="wizard-model-badge recommended">Recommended</span>';
                }
                if (model.hardwareTag) {
                    badges += `<span class="wizard-model-badge npu">${model.hardwareTag}</span>`;
                }

                card.innerHTML = `
                    <div class="wizard-model-radio"></div>
                    <div class="wizard-model-info">
                        <div class="wizard-model-name">Tencent HY-MT 1.5${badges}</div>
                        ${model.description ? `<div class="setting-hint">${escapeHtml(model.description)}</div>` : ''}
                        ${model.downloadSize ? `<div class="setting-hint">Download: ${escapeHtml(model.downloadSize)}</div>` : ''}
                    </div>
                `;

                card.addEventListener('click', () => {
                    document.querySelectorAll('.wizard-model-card').forEach(c => c.classList.remove('selected'));
                    card.classList.add('selected');
                    state.selectedModel = model.id;
                    $('btn-wizard-next').disabled = false;
                });

                listEl.appendChild(card);
            });

            updateButtons();
        } catch (e) {
            console.error('Failed to load models:', e);
            listEl.innerHTML = `
                <div class="wizard-error-box">
                    <strong>Failed to load models.</strong>
                    <p>${escapeHtml(e.message || String(e))}</p>
                </div>
            `;
        }
    }

    async function doDownloadModel() {
        if (!state.selectedModel || state.downloadInProgress) return;

        state.downloadInProgress = true;
        const btnNext = $('btn-wizard-next');
        btnNext.disabled = true;
        btnNext.textContent = 'Downloading...';
        hide('btn-wizard-skip');

        $('downloading-model-name').textContent = state.selectedModel;
        show('download-progress');
        show('wizard-terminal');
        $('terminal-output').textContent = '';

        try {
            await TauriBridge.invoke('wizard_download_model', {
                modelId: state.selectedModel,
                cacheDir: $('cache-dir-path').value.trim() || null,
            });
            // Note: success/failure handled by wizard-download-complete event
        } catch (e) {
            console.error('Download command failed:', e);
            appendTerminalLine('Error: ' + (e.message || String(e)), true);
            state.downloadInProgress = false;
            btnNext.disabled = false;
            btnNext.textContent = 'Retry';
        }
    }

    // =========================================================================
    // STEP 4: VERIFY & DONE
    // =========================================================================

    async function initStepVerify() {
        show('verify-progress');
        hide('verify-success');
        hide('verify-error');

        try {
            // Start the service
            await TauriBridge.invoke('wizard_start_service');

            // Brief wait for model warmup
            await new Promise(r => setTimeout(r, 2000));

            // Full probe: check CLI + service + model availability
            const status = await TauriBridge.invoke('refresh_foundry_local_status');
            const test = await TauriBridge.invoke('wizard_test_translation', {
                sourceText: $('test-source-text').value.trim(),
                sourceLanguage: 'zh-CN',
                targetLanguage: 'en-US',
            });

            hide('verify-progress');

            // Require CLI available AND service running (not just cliAvailable)
            const ready = status && status.serviceRunning && status.phase === 'ready'
                && test && test.translatedText;

            if (ready) {
                show('verify-success');

                // Build summary
                const summary = $('setup-summary');
                let rows = '';
                if (state.selectedModel) {
                    rows += '<div class="wizard-summary-row"><span class="wizard-summary-label">Engine</span><span class="wizard-summary-value">Tencent HY-MT 1.5</span></div>';
                }
                if (status.phase) {
                    rows += `<div class="wizard-summary-row"><span class="wizard-summary-label">Status</span><span class="wizard-summary-value">${escapeHtml(status.phase)}</span></div>`;
                }
                summary.innerHTML = rows;
                $('translation-test-result').innerHTML = `
                    <div class="wizard-summary-row"><span class="wizard-summary-label">Translation</span><span class="wizard-summary-value">${escapeHtml(test.translatedText)}</span></div>
                    <div class="wizard-summary-row"><span class="wizard-summary-label">Latency</span><span class="wizard-summary-value">${test.latencyMs} ms</span></div>
                `;
            } else {
                show('verify-error');
                const reason = !status?.cliAvailable
                    ? 'The local translation runtime is not installed.'
                    : !status?.serviceRunning
                        ? 'The Local Translation Engine is not running. Try again.'
                        : 'The engine is not fully ready. Check the support code and retry.';
                $('verify-error-message').textContent = reason;
            }
        } catch (e) {
            hide('verify-progress');
            show('verify-error');
            $('verify-error-message').textContent = e.message || String(e);
        }

        updateButtons();
    }

    // =========================================================================
    // TERMINAL OUTPUT
    // =========================================================================

    function appendTerminalLine(text, isError) {
        const terminal = $('terminal-output');
        if (!terminal) return;

        if (isError) {
            const span = document.createElement('span');
            span.className = 'line-error';
            span.textContent = text + '\n';
            terminal.appendChild(span);
        } else {
            terminal.textContent += text + '\n';
        }

        // Auto-scroll
        const container = $('wizard-terminal');
        if (container) {
            container.scrollTop = container.scrollHeight;
        }
    }

    // =========================================================================
    // EVENT LISTENERS
    // =========================================================================

    function resetWizardState() {
        state.currentStep = 1;
        state.selectedModel = null;
        state.modelDownloaded = false;
        state.downloadInProgress = false;
        $('terminal-output').textContent = '';
        showStep(1);
    }

    function setupEventListeners() {
        // Reset wizard to step 1 when re-opened (window is hidden, not destroyed)
        TauriBridge.event.listen('wizard-reset', () => {
            resetWizardState();
        });

        // Streaming output from model download
        TauriBridge.event.listen('wizard-output', (event) => {
            const { stream, line } = event.payload;
            appendTerminalLine(line, stream === 'stderr');
        });

        // Download complete
        TauriBridge.event.listen('wizard-download-complete', (event) => {
            const { success, error } = event.payload;
            state.downloadInProgress = false;

            if (success) {
                state.modelDownloaded = true;
                appendTerminalLine('Download complete!', false);
                hide('download-progress');
                // Auto-advance to verification
                setTimeout(() => showStep(4), 500);
            } else {
                appendTerminalLine('Download failed: ' + (error || 'Unknown error'), true);
                const btnNext = $('btn-wizard-next');
                btnNext.disabled = false;
                btnNext.textContent = 'Retry';
            }
        });
    }

    // =========================================================================
    // BUTTON HANDLERS
    // =========================================================================

    function setupButtons() {
        // Next button
        $('btn-wizard-next').addEventListener('click', async () => {
            switch (state.currentStep) {
                case 1:
                    showStep(2);
                    break;
                case 2:
                    showStep(3);
                    break;
                case 3:
                    if (state.downloadInProgress) return;
                    await doDownloadModel();
                    break;
                case 4:
                    // Close wizard
                    await TauriBridge.invoke('close_foundry_wizard', {
                        modelDownloaded: state.modelDownloaded,
                        selectedModel: state.selectedModel,
                    });
                    break;
            }
        });

        // Skip button
        $('btn-wizard-skip').addEventListener('click', () => {
            if (state.currentStep === 2) {
                showStep(3);
            } else if (state.currentStep === 3) {
                showStep(4);
            }
        });

        // Cancel button
        $('btn-wizard-cancel').addEventListener('click', async () => {
            await TauriBridge.invoke('close_foundry_wizard', {
                modelDownloaded: false,
                selectedModel: null,
            });
        });

        // Retry button
        $('btn-wizard-retry').addEventListener('click', () => {
            showStep(state.currentStep);
        });

        // Browse cache directory
        $('btn-browse-cache').addEventListener('click', handleBrowseCache);
    }

    // =========================================================================
    // UTILITY
    // =========================================================================

    function escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    // =========================================================================
    // INITIALIZATION
    // =========================================================================

    document.addEventListener('DOMContentLoaded', () => {
        setupButtons();
        setupEventListeners();
        showStep(1);
    });
})();
