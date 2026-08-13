/* global WizardState */

(function () {
  "use strict";

  const state = {
    step: 1,
    installing: false,
    installedThisRun: false,
  };

  const $ = (id) => document.getElementById(id);

  function setHidden(element, hidden) {
    if (element) element.hidden = hidden;
  }

  function showStep(step) {
    state.step = step;
    ["step-about", "step-install", "step-ready"].forEach((id, index) => {
      setHidden($(id), index + 1 !== step);
    });
    document.querySelectorAll(".wizard-step-item").forEach((item, index) => {
      item.classList.toggle("active", index + 1 === step);
      item.classList.toggle("done", index + 1 < step);
    });
    document.querySelectorAll(".wizard-step-connector").forEach((item, index) => {
      item.classList.toggle("done", index + 1 < step);
    });

    const primary = $("btn-primary");
    const cancel = $("btn-cancel");
    primary.disabled = state.installing;
    cancel.disabled = state.installing;
    if (step === 1) primary.textContent = "Install translation engine";
    if (step === 2) primary.textContent = state.installing ? "Installing…" : "Try again";
    if (step === 3) {
      primary.textContent = "Close";
      setHidden(cancel, true);
    } else {
      setHidden(cancel, false);
    }
    $(`step-${["about", "install", "ready"][step - 1]}`)
      ?.querySelector("h1")
      ?.focus();
  }

  function appendDetail(line, isError = false) {
    const output = $("setup-output");
    const text = String(line || "").trim();
    if (!text) return;
    output.textContent += `${isError ? "Error: " : ""}${text}\n`;
    output.scrollTop = output.scrollHeight;
  }

  function showInstallError(error) {
    state.installing = false;
    setHidden($("install-progress"), true);
    setHidden($("install-error"), false);
    const code = WizardState.supportCode(error);
    $("install-error-message").textContent =
      "Setup could not finish. Check your connection and storage, then try again.";
    $("install-support-code").textContent = code;
    appendDetail(error?.message || error || code, true);
    showStep(2);
  }

  async function installEngine() {
    if (state.installing) return;
    state.installing = true;
    $("setup-output").textContent = "";
    $("install-status").textContent = "Checking this PC…";
    $("install-detail").textContent = "Preparing the supported engine.";
    setHidden($("install-error"), true);
    setHidden($("install-progress"), false);
    showStep(2);

    try {
      await TauriBridge.invoke("wizard_install_engine");
    } catch (error) {
      showInstallError(error);
    }
  }

  async function verifyReady() {
    try {
      await TauriBridge.invoke("wizard_start_service");
      const status = await TauriBridge.invoke("refresh_engine_status");
      if (!WizardState.isReady(status)) {
        throw new Error("ENGINE_NOT_READY");
      }
      const test = await TauriBridge.invoke("wizard_test_translation", {
        sourceText: "先不提时钟塔",
        sourceLanguage: "zh-CN",
        targetLanguage: "en-US",
      });
      if (!test?.translatedText) {
        throw new Error("ENGINE_SAMPLE_TRANSLATION_FAILED");
      }
      $("sample-result").textContent = test.translatedText;
      $("sample-latency").textContent = `${test.latencyMs} ms`;
      showStep(3);
    } catch (error) {
      showInstallError(error);
    }
  }

  async function closeWizard() {
    await TauriBridge.invoke("close_engine_wizard", {
      modelDownloaded: state.installedThisRun,
      selectedModel: null,
    });
  }

  function resetWizard() {
    state.step = 1;
    state.installing = false;
    state.installedThisRun = false;
    $("setup-output").textContent = "";
    setHidden($("install-error"), true);
    showStep(1);
  }

  function setupEvents() {
    TauriBridge.event.listen("wizard-reset", resetWizard);
    TauriBridge.event.listen("wizard-output", (event) => {
      const { stream, line } = event.payload || {};
      appendDetail(line, stream === "stderr");
      if (line) {
        $("install-status").textContent = WizardState.progressMessage(line);
        $("install-detail").textContent = "Files are verified before they become active.";
      }
    });
    TauriBridge.event.listen("wizard-download-complete", async (event) => {
      state.installing = false;
      const { success, error } = event.payload || {};
      if (!success) {
        showInstallError(error || "ENGINE_SETUP_FAILED");
        return;
      }
      state.installedThisRun = true;
      $("install-status").textContent = "Translation engine installed";
      $("install-detail").textContent = "Running the final sample translation…";
      await verifyReady();
    });
  }

  function setupButtons() {
    $("btn-primary").addEventListener("click", async () => {
      if (state.step === 3) {
        await closeWizard();
      } else {
        await installEngine();
      }
    });
    $("btn-cancel").addEventListener("click", closeWizard);
  }

  document.addEventListener("DOMContentLoaded", () => {
    setupButtons();
    setupEvents();
    showStep(1);
  });
})();
