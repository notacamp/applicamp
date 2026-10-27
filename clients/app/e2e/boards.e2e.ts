import { expect, test } from "@playwright/test";

import {
  callApi,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  latestSearchStatus,
  signIn,
} from "./helpers";

/** An invented profile (nobody real), enough for a search to score against. */
const PROFILE = {
  basics: { label: "Senior software engineer", location: "Lisbon, Portugal", profiles: [] },
  work: [
    { name: "Example Org", position: "Senior Engineer", startDate: "2020-01", highlights: [] },
  ],
  education: [],
  skills: [{ name: "Rust" }],
  languages: [{ language: "English", fluency: "Full professional" }],
  certificates: [],
  projects: [],
};
const GOALS = { roles: ["Staff engineer"], work_mode: ["remote"] };

test("the boards searches read are ranked by salary, a candidate waits with its check, the Scanned view shows a search, and a match credits its board", async ({
  page,
}) => {
  await signIn(page);
  expect(await callApi(page, "PUT", "/api/v1/profile", PROFILE)).toBe(200);
  expect(await callApi(page, "PUT", "/api/v1/goals", GOALS)).toBe(200);
  // A search, so the Scanned view has something to show (the fake board).
  const started = await callApi(page, "POST", "/api/v1/searches");
  expect([202, 409]).toContain(started);
  await expect.poll(() => latestSearchStatus(page), { timeout: 60_000 }).toBe("finished");

  // The Boards page, from the sidebar or the tab bar.
  await page.goto("/boards");
  await expectPageFillsTheScreen(page, "boards");
  const approved = page.getByTestId("boards-section-approved");
  await expect(approved).toContainText("Fake Remote Board");
  await expect(approved).toContainText("avg");
  const waiting = page.getByTestId("boards-section-waiting");
  await expect(waiting).toContainText("Fake Waiting Board");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // A candidate shows its check; the demo person owns the boards.
  await waiting.getByTestId("boards-row").first().click();
  await expect(page.getByTestId("board-peek")).toBeVisible();
  await expect(page.getByTestId("board-terms-sentences")).toContainText(
    "You may not scrape this site",
  );
  await expect(page.getByTestId("board-sample")).toContainText("Senior Backend Engineer");
  await expect(page.getByTestId("board-approve")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await page.getByTestId("board-peek-close").click();

  // Scanned: the search, with the board's line.
  await page.getByTestId("boards-tab-scanned").click();
  const run = page.getByTestId("scanned-run").first();
  await expect(run).toBeVisible();
  await expect(run.getByTestId("scanned-line").first()).toContainText("Fake Remote Board");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // A match credits the board it came from, with a link to the posting there.
  await page.goto("/pipeline");
  await page.getByTestId("pipeline-stage-filter").selectOption("all");
  const row = page.getByTestId("pipeline-row").first();
  await expect(row).toBeVisible();
  const phone = (page.viewportSize()?.width ?? 0) < 768;
  await (phone ? row.click() : row.getByTestId("pipeline-row-open").click());
  await expect(page.getByTestId("peek-source-link").first()).toContainText("Fake Remote Board");
  await expect(page.getByTestId("peek-source-link").first()).toHaveAttribute(
    "href",
    /^https:\/\/example\.com\/remote-board\/jobs\//,
  );
  await expect(page.getByTestId("peek-apply")).toHaveAttribute(
    "href",
    /^https:\/\/example\.com\/apply\/\d+\?utm_source=api$/,
  );
});
