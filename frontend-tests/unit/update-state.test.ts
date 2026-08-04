import { describe, expect, it } from "vitest";
import {
  advanceDownload,
  deriveUpdatePresentation,
  downloadPercent,
  NO_DOWNLOAD,
  type DownloadProgress,
} from "../../src/ui/update-state";

describe("download progress", () => {
  // `Progress` carries one chunk's length, not a running total. Treating it as
  // a position pins the bar near zero for the whole download.
  it("accumulates chunk lengths rather than replacing the position", () => {
    const started = advanceDownload(NO_DOWNLOAD, {
      event: "Started",
      data: { contentLength: 1000 },
    });
    const first = advanceDownload(started, { event: "Progress", data: { chunkLength: 250 } });
    const second = advanceDownload(first, { event: "Progress", data: { chunkLength: 250 } });

    expect(downloadPercent(first)).toBe(25);
    expect(downloadPercent(second)).toBe(50);
  });

  it("reports no percentage when the server never declared a size", () => {
    const started = advanceDownload(NO_DOWNLOAD, { event: "Started", data: {} });
    const progressed = advanceDownload(started, { event: "Progress", data: { chunkLength: 900 } });

    expect(started.total).toBeNull();
    expect(downloadPercent(progressed)).toBeNull();
  });

  it("treats a zero content length as unknown rather than as a full bar", () => {
    const started = advanceDownload(NO_DOWNLOAD, {
      event: "Started",
      data: { contentLength: 0 },
    });

    expect(downloadPercent(started)).toBeNull();
  });

  it("finishes at the declared total even if chunk lengths did not add up", () => {
    const started = advanceDownload(NO_DOWNLOAD, {
      event: "Started",
      data: { contentLength: 1000 },
    });
    const short = advanceDownload(started, { event: "Progress", data: { chunkLength: 10 } });

    expect(downloadPercent(advanceDownload(short, { event: "Finished" }))).toBe(100);
  });

  it("never reports beyond a full bar when more arrives than declared", () => {
    const over: DownloadProgress = { received: 1500, total: 1000 };

    expect(downloadPercent(over)).toBe(100);
  });

  it("starts a second download from zero instead of resuming the first", () => {
    const finished: DownloadProgress = { received: 1000, total: 1000 };

    const restarted = advanceDownload(finished, {
      event: "Started",
      data: { contentLength: 2000 },
    });

    expect(restarted).toEqual({ received: 0, total: 2000 });
  });
});

describe("update presentation", () => {
  it("offers the install action only once a version is actually available", () => {
    expect(deriveUpdatePresentation({ kind: "idle" }, "0.6.6").action).toBe("check");
    expect(deriveUpdatePresentation({ kind: "upToDate" }, "0.6.6").action).toBe("check");
    expect(
      deriveUpdatePresentation({ kind: "available", version: "0.6.7", notes: null }, "0.6.6")
        .action,
    ).toBe("install");
  });

  it("disables the button through every step that must not be interrupted", () => {
    const busy = [
      { kind: "checking" } as const,
      { kind: "downloading", version: "0.6.7", percent: 12 } as const,
      { kind: "installing", version: "0.6.7" } as const,
    ];

    for (const status of busy) {
      const presentation = deriveUpdatePresentation(status, "0.6.6");
      expect(presentation.actionDisabled).toBe(true);
      expect(presentation.action).toBe("none");
    }
  });

  it("keeps a failed check retryable and shows why it failed", () => {
    const presentation = deriveUpdatePresentation(
      { kind: "error", message: "network unreachable" },
      "0.6.6",
    );

    expect(presentation.action).toBe("check");
    expect(presentation.actionDisabled).toBe(false);
    expect(presentation.detail).toContain("network unreachable");
  });

  it("surfaces release notes only when there is a version to decide about", () => {
    expect(
      deriveUpdatePresentation({ kind: "available", version: "0.6.7", notes: "Fixes" }, "0.6.6")
        .notes,
    ).toBe("Fixes");
    expect(deriveUpdatePresentation({ kind: "upToDate" }, "0.6.6").notes).toBeNull();
  });

  it("says the download size is unknown instead of showing a fake percentage", () => {
    const presentation = deriveUpdatePresentation(
      { kind: "downloading", version: "0.6.7", percent: null },
      "0.6.6",
    );

    expect(presentation.detail).not.toContain("%");
  });

  // Browser mode has no installation to replace, so the section must not offer
  // an action that would throw the moment it is pressed.
  it("offers nothing when the app cannot update itself", () => {
    const presentation = deriveUpdatePresentation({ kind: "unsupported" }, null);

    expect(presentation.action).toBe("none");
    expect(presentation.actionDisabled).toBe(true);
  });

  it("does not claim a version it was never told", () => {
    expect(deriveUpdatePresentation({ kind: "idle" }, null).detail).toBe(
      "Installed version unknown",
    );
  });
});
