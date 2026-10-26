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
    {
      name: "Example Org",
      position: "Senior Engineer",
      startDate: "2020-01",
      highlights: ["Ran the platform team's services"],
    },
  ],
  education: [],
  skills: [{ name: "Rust" }, { name: "PostgreSQL" }],
  languages: [{ language: "English", fluency: "Full professional" }],
  certificates: [],
  projects: [],
};

/** Goals a search keeps to: the fake board's jobs are remote within Europe. */
const GOALS = { roles: ["Staff engineer"], work_mode: ["remote"] };

test("someone with a profile and goals presses Search now, sees the matches arrive as Found, opens one and shortlists it", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);

  // Both projects share one stack and one person, in any order: set the
  // profile and goals they search with.
  expect(await callApi(page, "PUT", "/api/v1/profile", PROFILE)).toBe(200);
  expect(await callApi(page, "PUT", "/api/v1/goals", GOALS)).toBe(200);

  // Today knows the profile and goals are done: it never asks to set them up.
  await page.goto("/");
  await expectPageFillsTheScreen(page, "today");
  await expect(page.getByTestId("today-loading")).toHaveCount(0);
  await expect(page.getByTestId("today-load-error")).toHaveCount(0);
  await expect(page.getByTestId("set-up-profile")).toHaveCount(0);
  await expect(page.getByTestId("today-goals")).toHaveCount(0);

  // The Pipeline: Search now (confirmed if the estimate asks), then its progress.
  await page.goto("/pipeline");
  await expectPageFillsTheScreen(page, "pipeline");
  await expectAccessible(page);
  const posted = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname === "/api/v1/searches",
  );
  await page.getByTestId("pipeline-search-now").click();
  const confirm = page.getByTestId("search-confirm");
  await Promise.race([posted, confirm.waitFor().catch(() => undefined)]);
  if (await confirm.isVisible()) {
    await confirm.click();
  }
  expect((await posted).status()).toBe(202);
  // The search runs in the background; it ends with its line: jobs read and matched.
  await expect.poll(() => latestSearchStatus(page), { timeout: 60_000 }).toBe("finished");
  await expect(page.getByTestId("search-line")).toBeVisible();
  await expect(page.getByTestId("search-line")).toContainText(/\d/);
  await expect(page.getByTestId("search-stopped")).toHaveCount(0);

  // The matches are Found: a table on a desktop, cards on a phone.
  await page.getByTestId("pipeline-stage-filter").selectOption("found");
  const firstRow = page.getByTestId("pipeline-row").first();
  await expect(firstRow).toBeVisible();
  await expect(page.getByTestId(phone ? "pipeline-cards" : "pipeline-table")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await (phone ? firstRow.click() : firstRow.getByTestId("pipeline-row-open").click());

  // The peek: why it matches, its gaps, and its stage, changed to Shortlisted.
  const peek = page.getByTestId("peek");
  await expect(peek).toBeVisible();
  await expect(page.getByTestId("peek-why")).toBeVisible();
  await expect(page.getByTestId("peek-gaps")).toBeVisible();
  await expect(page.getByTestId("peek-open-posting")).toHaveAttribute("href", /^https?:\/\//);
  await expectAccessible(page);
  await expectNoSidewaysScroll(page);
  await page.getByTestId("peek-stage").selectOption("shortlisted");
  await expect(page.getByTestId("peek-stage")).toHaveValue("shortlisted");
  await page.getByTestId("peek-close").click();
  await expect(peek).toHaveCount(0);

  // Under Shortlisted, in the table and on the board.
  await page.getByTestId("pipeline-stage-filter").selectOption("shortlisted");
  await expect(page.getByTestId("pipeline-row").first()).toBeVisible();
  await page.getByTestId("pipeline-stage-filter").selectOption("all");
  await page.getByTestId("pipeline-view-board").click();
  await expect(page.getByTestId("pipeline-board")).toBeVisible();
  await expect(
    page.getByTestId("pipeline-column-shortlisted").getByTestId("pipeline-card").first(),
  ).toBeVisible();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Today shows the latest search's line.
  await page.goto("/");
  await expect(page.getByTestId("today-latest-search")).toBeVisible();
  await expectNoSidewaysScroll(page);
});
