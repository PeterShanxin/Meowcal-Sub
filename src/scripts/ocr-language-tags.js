/* global module */

(function exposeOcrLanguageTags(root, factory) {
  const api = factory();

  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }

  if (root) {
    root.OcrLanguageTags = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, () => {
  function normalizeOcrLanguageTag(tag) {
    return String(tag ?? "")
      .trim()
      .replaceAll("_", "-")
      .toLowerCase();
  }

  function getOcrLanguageFamily(tag) {
    const normalized = normalizeOcrLanguageTag(tag);
    const parts = normalized.split("-");

    if (parts[0] !== "zh") {
      return normalized;
    }

    if (normalized === "zh-cn" || parts.includes("hans")) {
      return "zh-hans";
    }

    if (normalized === "zh-tw" || parts.includes("hant")) {
      return "zh-hant";
    }

    return normalized;
  }

  function isOcrLanguageAvailable(installedTags, requestedTag) {
    const requestedFamily = getOcrLanguageFamily(requestedTag);

    if (!requestedFamily || !installedTags) {
      return false;
    }

    for (const installedTag of installedTags) {
      if (getOcrLanguageFamily(installedTag) === requestedFamily) {
        return true;
      }
    }

    return false;
  }

  return {
    isOcrLanguageAvailable,
    normalizeOcrLanguageTag,
  };
});
