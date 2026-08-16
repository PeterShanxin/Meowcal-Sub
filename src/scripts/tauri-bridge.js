// =============================================================================
// TAURI-BRIDGE.JS - Unified API Bridge for Tauri and Browser Modes
// =============================================================================
// This module provides a unified interface for calling backend commands.
// - In Tauri mode: Uses native IPC (window.__TAURI__.core.invoke)
// - In Browser mode: Uses HTTP API (fetch to localhost:3001)
//
// This allows the frontend to work in both environments without changes.
// =============================================================================

(function() {
    'use strict';

    // Detect if we're running inside Tauri's WebView
    const isTauri = !!(window.__TAURI__ && window.__TAURI__.core);

    // HTTP API base URL (used in browser mode).
    //
    // The default matches the backend's own default port. The override exists
    // because a fixed port makes browser verification a shared-machine hazard:
    // the automated smoke picks a port it has confirmed is free and tells both
    // sides about it, rather than requiring 3001 to be available (#35). It is
    // read from a global set before page scripts run - deliberately not from the
    // URL, so no link can repoint a running page at another origin.
    const API_BASE = typeof window.__MEOWCAL_API_BASE__ === 'string' && window.__MEOWCAL_API_BASE__
        ? window.__MEOWCAL_API_BASE__
        : 'http://localhost:3001/api';

    // Commands that are not available in browser mode (require Tauri window APIs)
    const TAURI_ONLY_COMMANDS = [
        'open_area_selector',
        'close_area_selector',
        'start_translation',
        'stop_translation',
    ];

    // =============================================================================
    // COMMAND ROUTING
    // =============================================================================

    /**
     * Map Tauri command names to HTTP endpoints for browser dev mode.
     *
     * COUPLING NOTE: These command names must match the #[tauri::command] function
     * names in src-tauri/src/commands.rs. The HTTP paths must match the routes in
     * src-tauri/src/http_server.rs. When adding a new command:
     *   1. Add #[tauri::command] function in commands.rs
     *   2. Add route in http_server.rs
     *   3. Add mapping here
     */
    const COMMAND_TO_ENDPOINT = {
        // System
        'get_system_info': { method: 'GET', path: '/system-info' },

        // Settings
        'get_settings': { method: 'GET', path: '/settings' },
        'save_settings': { method: 'POST', path: '/settings' },

        // Translation diagnostics
        'get_translation_diagnostics': { method: 'GET', path: '/translation/diagnostics' },
        'list_translation_backends': { method: 'GET', path: '/translation/diagnostics' },
        'translate_once': { method: 'POST', path: '/translation/translate' },

        // Local translation engine
        'list_engine_models': { method: 'GET', path: '/engine/models' },
        'get_engine_status': { method: 'GET', path: '/engine/status' },
        'refresh_engine_status': { method: 'POST', path: '/engine/refresh' },
        'prepare_engine': { method: 'POST', path: '/engine/prepare' },
        'make_engine_ready': { method: 'POST', path: '/engine/make-ready' },

        // OCR language management
        'get_ocr_languages': { method: 'GET', path: '/ocr/languages' },
        'install_ocr_language': { method: 'POST', path: '/ocr/install-language' },

        // Capture region
        'get_capture_region': { method: 'GET', path: '/capture-region' },
        'set_capture_region': { method: 'POST', path: '/capture-region' },

        // Curated engine setup wizard (Tauri-only, returns 501 in browser mode)
        'open_engine_wizard':           { method: 'POST', path: '/wizard/open' },
        'close_engine_wizard':          { method: 'POST', path: '/wizard/close' },
        'wizard_install_engine':         { method: 'POST', path: '/wizard/install-engine' },
        'wizard_start_service':          { method: 'POST', path: '/wizard/start-service' },
        'wizard_test_translation':       { method: 'POST', path: '/wizard/test-translation' },

        // Tauri-only (will return graceful error)
        'open_area_selector': { method: 'POST', path: '/area-selector' },
        'start_translation': { method: 'POST', path: '/translation/start' },
        'stop_translation': { method: 'POST', path: '/translation/stop' },
    };

    // =============================================================================
    // HTTP API CLIENT
    // =============================================================================

    /**
     * Make an HTTP request to the backend API
     */
    async function httpRequest(method, path, body = null) {
        const url = API_BASE + path;
        const options = {
            method,
            headers: {
                'Content-Type': 'application/json',
            },
        };

        if (body !== null) {
            options.body = JSON.stringify(body);
        }

        const response = await fetch(url, options);

        if (!response.ok) {
            const errorData = await response.json().catch(() => ({}));
            if (errorData.browserMode) {
                throw new Error(errorData.message || 'Feature not available in browser mode');
            }
            throw new Error(`HTTP ${response.status}: ${errorData.error || response.statusText}`);
        }

        return response.json();
    }

    /**
     * Invoke a command via HTTP API (browser mode)
     */
    async function httpInvoke(command, args = {}) {
        const mapping = COMMAND_TO_ENDPOINT[command];

        if (!mapping) {
            console.warn(`[TauriBridge] Unknown command: ${command}`);
            throw new Error(`Unknown command: ${command}`);
        }

        // Handle special response transformations
        const requestBody = command === 'save_settings' && args.settings
            ? args.settings
            : args;
        let result = await httpRequest(
            mapping.method,
            mapping.path,
            mapping.method === 'POST' ? requestBody : null,
        );

        // Transform responses to match Tauri format
        if (command === 'list_engine_models') {
            // HTTP returns { models: [...] }, Tauri returns [...]
            return result.models || [];
        }

        if (command === 'list_translation_backends') {
            // Return backends array from diagnostics
            return result.backends || [];
        }

        return result;
    }

    // =============================================================================
    // UNIFIED INVOKE FUNCTION
    // =============================================================================

    /**
     * Invoke a backend command (works in both Tauri and browser modes)
     *
     * @param {string} command - The command name (e.g., 'get_settings')
     * @param {object} args - Optional arguments for the command
     * @returns {Promise<any>} - The command result
     */
    async function invoke(command, args = {}) {
        if (isTauri) {
            // Use native Tauri IPC
            return window.__TAURI__.core.invoke(command, args);
        }

        // Use HTTP API
        return httpInvoke(command, args);
    }

    // =============================================================================
    // EVENT SYSTEM (BROWSER MODE)
    // =============================================================================

    // In browser mode, events are not supported (no real-time updates)
    // We provide no-op implementations to prevent errors

    const eventListeners = new Map();

    /**
     * Listen for events (no-op in browser mode)
     */
    async function listen(eventName, callback) {
        if (isTauri) {
            return window.__TAURI__.event.listen(eventName, callback);
        }

        // Browser mode: store listener but never call it
        // (Could implement SSE/WebSocket for real-time updates in the future)
        console.log(`[TauriBridge] Event listener registered (browser mode, no real-time updates): ${eventName}`);

        if (!eventListeners.has(eventName)) {
            eventListeners.set(eventName, []);
        }
        eventListeners.get(eventName).push(callback);

        // Return an unlisten function (no-op)
        return () => {
            const listeners = eventListeners.get(eventName);
            if (listeners) {
                const index = listeners.indexOf(callback);
                if (index > -1) {
                    listeners.splice(index, 1);
                }
            }
        };
    }

    /**
     * Emit an event (no-op in browser mode)
     */
    async function emit(eventName, payload) {
        if (isTauri) {
            return window.__TAURI__.event.emit(eventName, payload);
        }

        // Browser mode: trigger local listeners (for testing)
        const listeners = eventListeners.get(eventName);
        if (listeners) {
            listeners.forEach(callback => callback({ payload }));
        }
    }

    // =============================================================================
    // BROWSER MODE DETECTION & UI
    // =============================================================================

    /**
     * Check if running in browser mode
     */
    function isBrowserMode() {
        return !isTauri;
    }

    /**
     * Show browser mode indicator in the UI
     */
    function showBrowserModeIndicator() {
        if (!isBrowserMode()) return;

        // Create the indicator element
        const indicator = document.createElement('div');
        indicator.id = 'browser-mode-indicator';
        indicator.innerHTML = `
            <span class="browser-mode-badge">BROWSER MODE</span>
            <span class="browser-mode-hint">Some features unavailable</span>
        `;
        indicator.style.cssText = `
            position: fixed;
            bottom: 10px;
            right: 10px;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 8px 12px;
            border-radius: 8px;
            font-size: 12px;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            z-index: 9999;
            pointer-events: none;
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
            display: flex;
            flex-direction: column;
            align-items: flex-end;
            gap: 2px;
        `;

        // Style the badge
        const badge = indicator.querySelector('.browser-mode-badge');
        badge.style.cssText = `
            font-weight: 600;
            letter-spacing: 0.5px;
        `;

        // Style the hint
        const hint = indicator.querySelector('.browser-mode-hint');
        hint.style.cssText = `
            font-size: 10px;
            opacity: 0.8;
        `;

        // Add to page when DOM is ready
        if (document.body) {
            document.body.appendChild(indicator);
        } else {
            document.addEventListener('DOMContentLoaded', () => {
                document.body.appendChild(indicator);
            });
        }

        console.log('[TauriBridge] Browser mode active - using HTTP API');
    }

    // =============================================================================
    // EXPORTS
    // =============================================================================

    // The shell windows are undecorated, so the app draws its own title bar and
    // has to provide the controls the system strip used to give it. Browser mode
    // has no window to drive, so this stays undefined and the bar hides them.
    const currentWindow = () => window.__TAURI__?.window?.getCurrentWindow?.() || null;
    const windowControls = isTauri ? {
        minimize: async () => { await currentWindow()?.minimize(); },
        toggleMaximize: async () => { await currentWindow()?.toggleMaximize(); },
        close: async () => { await currentWindow()?.close(); },
        isMaximized: async () => (await currentWindow()?.isMaximized()) === true,
    } : undefined;

    // In-app update. The plugins publish themselves on `window.__TAURI__`
    // because the app builds with `withGlobalTauri`, so there is nothing to
    // import - but a build without the plugins registered would leave these
    // undefined, and the Settings screen has to degrade rather than throw.
    //
    // `check()` resolves to null when the endpoint answered and this install is
    // already current. Older plugin builds returned an object with
    // `available: false` instead, so both are treated as "nothing to do".
    const updater = () => window.__TAURI__?.updater;
    const updates = isTauri && updater() ? {
        currentVersion: () => window.__TAURI__.app.getVersion(),
        check: async () => {
            const update = await updater().check();
            if (!update || update.available === false) {
                return null;
            }
            return {
                version: update.version,
                notes: update.body || null,
                install: (onProgress) => update.downloadAndInstall(onProgress),
            };
        },
        restart: () => window.__TAURI__.process.relaunch(),
    } : undefined;

    // Create the bridge object
    const TauriBridge = {
        windowControls,
        updates,

        // Core functions
        invoke,
        isTauri,
        isBrowserMode,

        // Event system
        event: {
            listen,
            emit,
        },

        // UI helpers
        showBrowserModeIndicator,

        // Constants
        API_BASE,
        TAURI_ONLY_COMMANDS,
    };

    // Expose globally
    window.TauriBridge = TauriBridge;

    // Auto-show browser mode indicator
    if (isBrowserMode()) {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', showBrowserModeIndicator);
        } else {
            showBrowserModeIndicator();
        }
    }

    console.log(`[TauriBridge] Initialized in ${isTauri ? 'Tauri' : 'Browser'} mode`);
})();
