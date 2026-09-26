// In-page stand-in for the Tauri runtime, used only by the UI audit capture
// script. It answers the same commands as src-tauri/src/commands.rs with
// in-memory state so the shell windows can be driven end to end on a machine
// that cannot build the Windows app. It proves presentation and flow, not
// capture, OCR, the selector window, or the overlay.
(() => {
  const scenario = Object.assign(
    {
      phase: "ready",
      ocr: ["en-US", "zh-Hans-CN"],
      region: null,
      selectorPicks: true,
      selectorDelayMs: 900,
      startDelayMs: 700,
      startError: null,
      testDelayMs: 400,
      wizardFailAt: null,
    },
    window.__MOCK_SCENARIO__ || {},
  );
  const key = "mock.state";
  const saved = JSON.parse(localStorage.getItem(key) || "null");
  const state = saved || {
    settings: null,
    region: scenario.region,
    phase: scenario.phase,
    ocr: scenario.ocr,
    running: false,
  };
  const persist = () => localStorage.setItem(key, JSON.stringify(state));
  persist();

  const listeners = new Map();
  const emit = (name, payload) =>
    (listeners.get(name) || []).slice().forEach((callback) => callback({ payload }));
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const engine = () => ({
    phase: state.phase,
    serviceRunning: state.phase === "ready",
    supportCode: state.phase === "error" ? "ENGINE_START_TIMEOUT" : undefined,
  });
  window.__MOCK_CALLS__ = [];

  const commands = {
    // The real backend always answers with a full config; the app merges defaults.
    get_settings: () => state.settings ?? { sourceLanguage: "zh-CN", targetLanguage: "en-US" },
    save_settings: ({ settings }) => {
      state.settings = settings;
    },
    get_ocr_languages: () => state.ocr,
    install_ocr_language: async ({ languageTag }) => {
      await sleep(600);
      state.ocr = [...state.ocr, languageTag];
    },
    get_engine_status: engine,
    refresh_engine_status: engine,
    make_engine_ready: async () => {
      await sleep(400);
      if (state.phase === "notRunning" || state.phase === "preparing") state.phase = "ready";
      return engine();
    },
    get_capture_region: () => state.region,
    is_translation_running: () => state.running,
    open_area_selector: () => {
      if (scenario.selectorPicks) {
        setTimeout(() => {
          state.region = { x: 320, y: 820, width: 1280, height: 120 };
          persist();
          emit("region-selected", state.region);
        }, scenario.selectorDelayMs);
      }
      return { mode: "legacy" };
    },
    start_translation: async () => {
      await sleep(scenario.startDelayMs);
      if (scenario.startError) throw scenario.startError;
      if (!state.region) throw "No capture region set. Please select an area first.";
      state.running = true;
    },
    stop_translation: async () => {
      await sleep(300);
      state.running = false;
    },
    open_engine_wizard: () => {},
    close_engine_wizard: () => {
      emit("engine-wizard-closed", null);
    },
    wizard_install_engine: async () => {
      const lines = ["Checking system", "Downloading model", "Verifying files", "Installing"];
      (async () => {
        for (const [index, line] of lines.entries()) {
          await sleep(500);
          if (scenario.wizardFailAt === index) {
            emit("wizard-download-complete", { success: false, error: "ENGINE_DOWNLOAD_FAILED" });
            return;
          }
          emit("wizard-output", { line, stream: "stdout" });
        }
        state.phase = "notRunning";
        persist();
        emit("wizard-download-complete", { success: true });
      })();
    },
    wizard_start_service: async () => {
      await sleep(400);
      state.phase = "ready";
    },
    wizard_test_translation: async () => {
      await sleep(scenario.testDelayMs);
      if (state.phase !== "ready") throw "ENGINE_NOT_READY";
      return { translatedText: "我们先别谈钟楼的事。", latencyMs: 412 };
    },
  };

  const invoke = async (command, args = {}) => {
    window.__MOCK_CALLS__.push(command);
    const handler = commands[command];
    if (!handler) throw `Unknown command: ${command}`;
    try {
      return await handler(args);
    } finally {
      persist();
    }
  };

  const noop = async () => {};
  window.__MOCK_EMIT__ = emit;
  window.__TAURI__ = {
    core: { invoke },
    event: {
      listen: async (name, callback) => {
        if (!listeners.has(name)) listeners.set(name, []);
        listeners.get(name).push(callback);
        return () =>
          listeners.set(
            name,
            listeners.get(name).filter((item) => item !== callback),
          );
      },
      emit: async (name, payload) => emit(name, payload),
    },
    window: {
      getCurrentWindow: () => ({
        minimize: noop,
        toggleMaximize: noop,
        close: noop,
        isMaximized: async () => false,
      }),
    },
    app: { getIdentifier: async () => "com.meowcal.sub", getVersion: async () => "0.8.6" },
    updater: { check: async () => null },
    process: { relaunch: noop },
  };
})();
