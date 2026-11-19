import { expect, test, type Page } from "@playwright/test";
import type { FeedbackPreview } from "../../src/ui/feedback-model";

const preview: FeedbackPreview = {
  snapshotId: "local-snapshot",
  version: "0.10.0",
  stages: [{ stage: "translation", status: "failed", elapsedMs: 1200, code: "TRANSLATION_FAILED" }],
  materials: [
    {
      id: "words",
      kind: "translation",
      label: "Source and translation",
      text: "Fixture line\n示例译文",
    },
    { id: "folder", kind: "path", label: "Session log path", text: "C:\\fixture\\session.log" },
  ],
};

async function start(page: Page) {
  await page.route("**/api/**", (route) => {
    const path = new URL(route.request().url()).pathname;
    const response =
      path === "/api/engine/status"
        ? { phase: "ready" }
        : path === "/api/settings"
          ? { uiLanguage: "en" }
          : path === "/api/ocr/languages"
            ? ["en-US"]
            : null;
    return route.fulfill({ json: response });
  });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Let’s watch" })).toBeVisible();
  await page.evaluate((data) => {
    const original = window.TauriBridge.invoke;
    const requests: Array<Record<string, unknown>> = [];
    const previews: Array<Record<string, unknown>> = [];
    Object.assign(window, { feedbackRequests: requests, feedbackPreviews: previews });
    window.TauriBridge.invoke = async <T>(
      command: string,
      args?: Record<string, unknown>,
    ): Promise<T> => {
      if (command === "feedback_preview") {
        previews.push(structuredClone(args!));
        return data as T;
      }
      if (command === "feedback_send") {
        requests.push(structuredClone(args!.request as Record<string, unknown>));
        if (requests.length === 1) throw new Error("FEEDBACK_OFFLINE");
        return { issueNumber: 184 } as T;
      }
      return original<T>(command, args);
    };
  }, preview);
}

test("feedback keeps optional material private and retries one immutable request", async ({
  page,
}) => {
  await start(page);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const entry = page.getByRole("button", { name: "Feedback", exact: true });
  await entry.click();
  const dialog = page.getByRole("dialog", { name: "Feedback" });
  await dialog.getByLabel("Feedback type").selectOption("translation");
  await dialog.getByRole("textbox").fill("A fixture translation reverses the meaning.");
  await expect(dialog.getByText("Fixture line", { exact: false })).toHaveCount(0);
  const materials = dialog.getByRole("switch", { name: "Attach troubleshooting materials" });
  await expect(materials).not.toBeChecked();
  await materials.check();
  await expect(dialog.locator("pre").first()).toHaveText("Fixture line\n示例译文");
  await dialog.getByRole("checkbox", { name: "Source and translation" }).check();
  await expect(dialog.getByRole("checkbox", { name: "Session log path" })).not.toBeChecked();
  const send = dialog.getByRole("button", { name: "Send", exact: true });
  await send.click();
  await expect(dialog.getByRole("alert")).toBeVisible();
  await send.click();
  await expect(dialog.getByRole("link", { name: /View issue #184/ })).toHaveAttribute(
    "href",
    "https://github.com/PeterShanxin/Meowcal-Sub/issues/184",
  );
  const requests = await page.evaluate(
    () =>
      (window as unknown as { feedbackRequests: Array<Record<string, unknown>> }).feedbackRequests,
  );
  expect(requests).toHaveLength(2);
  expect(requests[1]).toEqual(requests[0]);
  expect(requests[0]).toMatchObject({
    materialIds: ["words"],
    snapshotId: "local-snapshot",
    type: "translation",
  });
  await page.keyboard.press("Escape");
  await expect(dialog).not.toBeVisible();
  await expect(entry).toBeFocused();
});

test("a capture error opens its prepared draft and can be sent without a description", async ({
  page,
}) => {
  await start(page);
  await page.evaluate(() =>
    window.TauriBridge.event.emit("capture-status", {
      isError: true,
      message: "Screen capture failed",
      faultCode: "CAPTURE_FAILED",
      feedbackToken: "17/5",
    }),
  );
  await page.getByRole("button", { name: "Report error" }).click();
  const dialog = page.getByRole("dialog");
  const previews = await page.evaluate(
    () =>
      (window as unknown as { feedbackPreviews: Array<Record<string, unknown>> }).feedbackPreviews,
  );
  expect(previews).toEqual([{ faultCode: "CAPTURE_FAILED", faultToken: "17/5" }]);
  await expect(dialog.getByLabel("Feedback type")).toHaveValue("error");
  await expect(dialog.getByRole("textbox")).not.toHaveAttribute("required", "");
  await dialog.getByRole("button", { name: "Send", exact: true }).click();
  await expect(dialog.getByRole("alert")).toBeVisible();
  await dialog.getByRole("button", { name: "Send", exact: true }).click();
  await expect(dialog.getByText("Feedback sent", { exact: true })).toBeVisible();
});

test("turning attachments off removes every selection from the sent draft", async ({ page }) => {
  await start(page);
  await page.getByRole("button", { name: "Feedback", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Feedback type").selectOption("ocr");
  await dialog.getByRole("textbox").fill("Fixture");
  const toggle = dialog.getByRole("switch");
  await toggle.check();
  await dialog.getByRole("checkbox", { name: "Session log path" }).check();
  await toggle.uncheck();
  await dialog.getByRole("button", { name: "Send", exact: true }).click();
  await expect(dialog.getByRole("alert")).toBeVisible();
  const requests = await page.evaluate(
    () =>
      (window as unknown as { feedbackRequests: Array<Record<string, unknown>> }).feedbackRequests,
  );
  expect(requests[0].materialIds).toEqual([]);
});
