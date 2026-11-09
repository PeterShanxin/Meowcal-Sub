import { test, expect } from "@playwright/test";

const fixtureUrl = new URL("../../evals/band-gate/index.html", import.meta.url).href;

for (const [scenarioId, text] of [
  ["rapid-dialogue-30s", "I can go."],
  ["single-character-12s", "好"],
  ["paused-near-match-30s", "The sailor is waiting here."],
]) {
  test(`${scenarioId} plays without negative controls`, async ({ page }) => {
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(fixtureUrl);
    await page.evaluate((id) => {
      window.fixture.setScenario(id);
      window.fixture.start();
    }, scenarioId);
    await expect(page.locator("#subtitle-card")).toHaveText(text);
    expect(await page.evaluate(() => window.fixture.readState().scenarioId)).toBe(scenarioId);
    expect(errors).toEqual([]);
  });
}
