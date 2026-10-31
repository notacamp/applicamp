import { expect, test, type Page } from "@playwright/test";

import {
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

// Filmed: CI keeps this spec's videos (the `e2e-recordings` artifact), the
// recording of the Agents page at desktop width and at 375px.
test.use({ video: "on" });

/** A beat for whoever watches the recording; nothing waits on it. */
const beat = (page: Page) => page.waitForTimeout(800);

test("Run a check: it ends succeeded, its steps show live, and its sandbox log shows a request outside its policy blocked", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);

  // The Agents page (the stack runs agents with the fake sandbox runtime:
  // deploy/compose.e2e.yml).
  await page.goto("/agents");
  await expectPageFillsTheScreen(page, "agents");
  await expect(page.getByTestId("runs-unavailable")).toHaveCount(0);
  await beat(page);

  // Run a check: the new run opens (alone, with the way back, on a phone).
  await page.getByTestId("run-check").click();
  await expect(page).toHaveURL(/\/agents\?run=[0-9a-f-]{36}$/);
  const status = page.getByTestId("run-status");
  await expect(status).toBeVisible();

  // Its steps arrive as it takes them, and it ends succeeded.
  const steps = page.getByTestId("run-log-agent");
  await expect(steps).toContainText("Read its context.", { timeout: 60_000 });
  await expect(steps).toContainText("Called probe_echo; it answered.", { timeout: 60_000 });
  await expect(status).toHaveAttribute("data-outcome", "succeeded", { timeout: 60_000 });
  await expect(status).toContainText("Succeeded");
  await expect(steps).toContainText("refused past its cap");
  await expect(steps).toContainText("example.com is blocked");
  await expect(page.getByTestId("run-stop")).toHaveCount(0);
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await beat(page);

  // The sandbox log: the request outside its policy, blocked.
  await page.getByTestId("run-tab-sandbox").click();
  const blocked = page
    .getByTestId("run-log-sandbox")
    .getByTestId("run-event")
    .filter({ hasText: "example.com:443" })
    .first();
  await expect(blocked).toHaveAttribute("data-level", "deny");
  await expect(blocked).toContainText("Blocked");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Its facts: where it ran and how it ended.
  await page.getByTestId("run-facts").scrollIntoViewIfNeeded();
  await expect(page.getByTestId("run-fact-end")).toContainText("Succeeded");
  await expect(page.getByTestId("run-fact-policy")).toContainText("probe");
  await expect(page.getByTestId("run-fact-sandbox")).not.toContainText("Not yet");
  await expect(page.getByTestId("run-fact-tokens")).toContainText("0 in · 0 out");
  await beat(page);

  // The run in the list: beside it on a desktop, back to it on a phone.
  const list = page.getByTestId("runs-list");
  if (phone) {
    await expect(list).toBeHidden();
    await page.getByTestId("runs-back").click();
    await expect(page).toHaveURL(/\/agents$/);
  }
  await expect(list).toBeVisible();
  const row = page.getByTestId("runs-row").first();
  await expect(row).toHaveAttribute("data-outcome", "succeeded");
  await expect(row).toContainText("Check");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await beat(page);
});
