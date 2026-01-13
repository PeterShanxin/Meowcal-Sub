// =============================================================================
// EDGE_TRANSLATOR.JS - Experimental Edge Translator Bridge
// =============================================================================
// This module wires WebView2 Translator API to Rust via Tauri events.
// It never evals user input and only uses structured payloads.
// =============================================================================

(function () {
    if (window.MeowcalEdgeTranslator) {
        return;
    }

    const translatorCache = new Map();
    let listenerRegistered = false;

    function getTranslationApi() {
        if (!window.navigator) {
            return null;
        }
        const api = navigator.translation;
        if (!api || typeof api.createTranslator !== 'function') {
            return null;
        }
        return api;
    }

    async function probeEdgeTranslator(sourceLanguage, targetLanguage) {
        const api = getTranslationApi();
        if (!api) {
            return {
                readyState: 'notSupported',
                notes: 'Translation API not available in this WebView2 runtime.',
            };
        }

        if (typeof api.canTranslate !== 'function') {
            return {
                readyState: 'notSupported',
                notes: 'Translation API present but canTranslate is missing (avoiding auto-download).',
            };
        }

        try {
            const status = await api.canTranslate({
                sourceLanguage,
                targetLanguage,
            });

            if (status === 'no') {
                return {
                    readyState: 'notSupported',
                    notes: 'Language pair not supported by the Translator API.',
                };
            }

            if (status === 'after-download') {
                return {
                    readyState: 'notReady',
                    notes: 'Model download required. Use Edge Translator once to cache the model.',
                };
            }

            return {
                readyState: 'ready',
                notes: 'Translator API available.',
            };
        } catch (error) {
            return {
                readyState: 'error',
                notes: `canTranslate failed: ${String(error)}`,
            };
        }
    }

    async function getTranslator(sourceLanguage, targetLanguage) {
        const cacheKey = `${sourceLanguage}::${targetLanguage}`;
        if (translatorCache.has(cacheKey)) {
            return translatorCache.get(cacheKey);
        }

        const api = getTranslationApi();
        if (!api) {
            return null;
        }

        const translator = await api.createTranslator({
            sourceLanguage,
            targetLanguage,
        });
        translatorCache.set(cacheKey, translator);
        return translator;
    }

    async function edgeTranslate(request) {
        const api = getTranslationApi();
        if (!api) {
            return {
                requestId: request.requestId,
                kind: 'translate',
                error: 'Translation API not available in this WebView2 runtime.',
            };
        }

        if (typeof api.canTranslate !== 'function') {
            return {
                requestId: request.requestId,
                kind: 'translate',
                error: 'Translation API missing canTranslate; refusing to trigger auto-download.',
            };
        }

        const check = await probeEdgeTranslator(request.sourceLanguage, request.targetLanguage);
        if (check.readyState !== 'ready') {
            return {
                requestId: request.requestId,
                kind: 'translate',
                error: check.notes || 'Translator API not ready.',
            };
        }

        try {
            const translator = await getTranslator(
                request.sourceLanguage,
                request.targetLanguage
            );
            if (!translator) {
                return {
                    requestId: request.requestId,
                    kind: 'translate',
                    error: 'Failed to create translator instance.',
                };
            }

            const translatedText = await translator.translate(request.text || '');
            return {
                requestId: request.requestId,
                kind: 'translate',
                translatedText,
            };
        } catch (error) {
            return {
                requestId: request.requestId,
                kind: 'translate',
                error: `Translate failed: ${String(error)}`,
            };
        }
    }

    async function prepareEdgeTranslator(sourceLanguage, targetLanguage) {
        const api = getTranslationApi();
        if (!api) {
            return {
                readyState: 'notSupported',
                notes: 'Translation API not available in this WebView2 runtime.',
            };
        }

        if (typeof api.canTranslate !== 'function') {
            return {
                readyState: 'notSupported',
                notes: 'Translation API missing canTranslate; refusing to trigger auto-download.',
            };
        }

        try {
            const status = await api.canTranslate({ sourceLanguage, targetLanguage });
            if (status === 'no') {
                return {
                    readyState: 'notSupported',
                    notes: 'Language pair not supported by the Translator API.',
                };
            }

            if (status === 'after-download') {
                await api.createTranslator({ sourceLanguage, targetLanguage });
                return {
                    readyState: 'notReady',
                    notes: 'Model download started. Check status again in a bit.',
                };
            }

            return {
                readyState: 'ready',
                notes: 'Model already available.',
            };
        } catch (error) {
            return {
                readyState: 'error',
                notes: `Prepare failed: ${String(error)}`,
            };
        }
    }

    async function handleRequest(payload) {
        if (!payload || !payload.requestId) {
            return {
                requestId: 'unknown',
                kind: 'error',
                error: 'Invalid request payload.',
            };
        }

        if (payload.kind === 'probe') {
            const result = await probeEdgeTranslator(
                payload.sourceLanguage,
                payload.targetLanguage
            );
            return {
                requestId: payload.requestId,
                kind: 'probe',
                readyState: result.readyState,
                notes: result.notes,
            };
        }

        if (payload.kind === 'translate') {
            return edgeTranslate(payload);
        }

        return {
            requestId: payload.requestId,
            kind: payload.kind,
            error: 'Unknown request kind.',
        };
    }

    async function registerEdgeTranslatorBridge() {
        if (listenerRegistered || !window.__TAURI__ || !window.__TAURI__.event) {
            return;
        }

        listenerRegistered = true;
        await window.__TAURI__.event.listen('edge-translate-request', async (event) => {
            const response = await handleRequest(event.payload);
            try {
                await window.__TAURI__.event.emit('edge-translate-response', response);
            } catch (error) {
                // Ignore emit errors to avoid noisy failures.
            }
        });
    }

    window.MeowcalEdgeTranslator = {
        probeEdgeTranslator,
        prepareEdgeTranslator,
        registerEdgeTranslatorBridge,
    };
})();
