import { expect, test } from "@playwright/test";

import {
  callApi,
  callApiJson,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  latestSearchStatus,
  signIn,
} from "./helpers";

// Filmed: CI keeps this spec's videos (the `e2e-recordings` artifact), the
// recording of #74 at 375px and on a desktop. Demo data only.
test.use({ video: "on" });

/** An invented profile (nobody real), enough for a search to score against. */
const PROFILE = {
  basics: { label: "Senior software engineer", location: "Lisbon, Portugal", profiles: [] },
  work: [
    { name: "Example Org", position: "Senior Engineer", startDate: "2020-01", highlights: [] },
  ],
  education: [],
  skills: [{ name: "Rails" }, { name: "Platform" }, { name: "Rust" }],
  languages: [{ language: "English", fluency: "Full professional" }],
  certificates: [],
  projects: [],
};
const GOALS = { roles: ["Staff engineer"], work_mode: ["remote"], autonomy: "find_only" };

test("the Searches tab says what each search read, read in full and found, why the rest was left out, and what it cost", async ({
  page,
}) => {
  await signIn(page);
  expect(await callApi(page, "PUT", "/api/v1/profile", PROFILE)).toBe(200);
  expect(await callApi(page, "PUT", "/api/v1/goals", GOALS)).toBe(200);
  // A search reads the job store, which the stack fills by itself from its
  // approved fake board: wait for it to hold jobs rather than sleep.
  await expect
    .poll(
      async () => {
        const { body } = await callApiJson(page, "GET", "/api/v1/boards/store");
        return (body as { jobs_stored?: number }).jobs_stored ?? 0;
      },
      { timeout: 120_000, intervals: [2000] },
    )
    .toBeGreaterThan(0);
  const started = await callApi(page, "POST", "/api/v1/searches");
  expect([202, 409, 429]).toContain(started);
  await expect.poll(() => latestSearchStatus(page), { timeout: 60_000 }).toBe("finished");

  await page.goto("/boards?tab=scanned");
  await expectPageFillsTheScreen(page, "boards");
  const run = page.getByTestId("scanned-run").first();
  await expect(run).toBeVisible();
  // Four numbers first: jobs read, read in full, good matches, cost.
  for (const figure of ["read", "read-in-full", "good-matches", "cost"]) {
    await expect(run.getByTestId(`scanned-${figure}`)).toContainText(/\d/);
  }
  await expect(run).toContainText("Where the matches came from");
  // The job open only to Germany and Austria is always left out: the list says why.
  await expect(run).toContainText("Why jobs were left out");
  await expect(run.getByTestId("scanned-left-out")).toContainText(" · ");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Each site behind Show by site; buttons big enough to tap on a phone.
  const bySite = run.getByTestId("scanned-by-site");
  const phone = (page.viewportSize()?.width ?? 0) < 768;
  const box = await bySite.boundingBox();
  expect(box?.height ?? 0).toBeGreaterThanOrEqual(phone ? 44 : 36);
  await bySite.click();
  await expect(run.getByTestId("scanned-sites")).toContainText("Fake Remote Board");
  await expectNoSidewaysScroll(page);

  // Search now, with no price in its label: the cost is on the line under it.
  const searchNow = page.getByTestId("scanned-search-now");
  await searchNow.scrollIntoViewIfNeeded();
  await expect(searchNow).toContainText("The app searches every night at 21:00.");
  await expect(page.getByTestId("scanned-search")).toHaveText("Search now");
  await expectAccessible(page);
});
