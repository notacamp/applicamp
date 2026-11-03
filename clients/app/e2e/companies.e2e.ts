import { expect, test, type Page } from "@playwright/test";

import {
  callApi,
  callApiJson,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

/**
 * Each Playwright project has its own invented companies (the fake's
 * northwind-<project>.test, contoso-<project>.test and tailspin-<project>.test),
 * so the two never watch the same one. The fake's remote board lists two jobs of
 * Northwind Stays for the skill `Watch-<project>`.
 */
const TITLES: Record<string, { matches: [string, string]; fit: string }> = {
  desktop: {
    matches: ["Senior Rails Engineer", "Backend Engineer"],
    fit: "Staff Platform Engineer",
  },
  phone: {
    matches: ["Senior Ruby Engineer", "Platform Engineer"],
    fit: "Principal Platform Engineer",
  },
};

/** An invented profile (nobody real) whose first skill the fake's remote board searches by. */
function profile(project: string) {
  return {
    basics: { label: "Senior software engineer", location: "Lisbon, Portugal", profiles: [] },
    work: [
      { name: "Example Org", position: "Senior Engineer", startDate: "2020-01", highlights: [] },
    ],
    education: [],
    skills: [{ name: `Watch-${project}` }, { name: "Rails" }, { name: "Rust" }],
    languages: [{ language: "English", fluency: "Full professional" }],
    certificates: [],
    projects: [],
  };
}
const GOALS = { roles: ["Staff engineer"], work_mode: ["remote"] };

/** The status of search `id`, as the latest-search answer says it (`null` when it is another). */
function statusOf(page: Page, id: string) {
  return page.evaluate(async (id) => {
    const answer = (await (await fetch("/api/v1/searches/latest")).json()) as {
      search?: { id: string; status: string } | null;
    };
    return answer.search?.id === id ? answer.search.status : null;
  }, id);
}

/**
 * Starts a search and waits until it is done. Every search also reads the
 * watched companies' job boards in this stack (`WATCH_EVERY_SECS` is 0), so
 * this is the daily check without waiting for the night.
 */
async function search(page: Page) {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    const started = await callApiJson(page, "POST", "/api/v1/searches");
    if (started.status === 202) {
      const { id } = started.body as { id: string };
      await expect.poll(() => statusOf(page, id), { timeout: 60_000 }).toBe("finished");
      return;
    }
    // Another search is running: let it end.
    await page.waitForTimeout(1000);
  }
  throw new Error("a search could not be started");
}

test("a company whose jobs keep fitting you is suggested with its reasons, watched, its job board found, and a new job that fits lands on Today", async ({
  page,
}, testInfo) => {
  test.setTimeout(180_000);
  const project = testInfo.project.name;
  const titles = TITLES[project];
  if (titles === undefined) {
    throw new Error("no titles for this project");
  }
  const northwind = `northwind-${project}.test`;
  await signIn(page);
  expect(await callApi(page, "PUT", "/api/v1/profile", profile(project))).toBe(200);
  expect(await callApi(page, "PUT", "/api/v1/goals", GOALS)).toBe(200);
  // Two matches from Northwind Stays, whose site the fake's board gives.
  await search(page);

  // Companies: a suggestion with the reasons, each traced to a posting.
  await page.goto("/companies");
  await expectPageFillsTheScreen(page, "companies");
  const card = page.getByTestId(/^company-card-/).filter({ hasText: northwind });
  await expect(card).toBeVisible({ timeout: 30_000 });
  await expect(card).toContainText("Northwind Stays");
  await expect(card).toContainText("fit you this week");
  await expect(card).not.toContainText(titles.matches[0]);
  await card.getByTestId(/^company-why-/).click();
  await expect(card).toContainText(`${titles.matches[0]} (fits`);
  await expect(card).toContainText(`${titles.matches[1]} (fits`);
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Watch: it moves to Watching, its board is found on its own site and read.
  await card.getByTestId(/^company-watch-/).click();
  const row = page.getByTestId(/^watching-row-/).filter({ hasText: northwind });
  await expect(row).toBeVisible();
  await expect(row.getByTestId(/^watching-board-/)).toContainText("Greenhouse", {
    timeout: 30_000,
  });
  await expect(row.getByTestId(/^watching-open-/)).toContainText("2 open jobs");
  await expect(row.getByTestId(/^watching-failed-/)).toHaveCount(0);
  await expectNoSidewaysScroll(page);

  // The daily check (a search here): one new job fits, one fails a hard requirement.
  await search(page);
  await page.goto("/");
  await expectPageFillsTheScreen(page, "today");
  const hit = page.getByTestId(/^today-hit-[\da-f-]{36}$/).filter({ hasText: titles.fit });
  await expect(hit).toBeVisible({ timeout: 30_000 });
  await expect(hit).toContainText("Northwind Stays posted a job that fits you");
  await expect(hit.getByTestId(/^today-hit-found-/)).toContainText("Found on their own job board");
  await expect(hit.getByTestId(/^today-hit-found-/)).toContainText("in Pipeline as Found");
  // The job open only to Germany and Austria was dropped, not suggested.
  await expect(page.getByTestId("today")).not.toContainText("(m/w/d)");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Open it: the Pipeline's peek, credited to the company's own board.
  await hit.getByTestId(/^today-hit-open-/).click();
  await expect(page).toHaveURL(/\/pipeline/);
  await expect(page.getByTestId("peek")).toBeVisible();
  await expect(page.getByTestId("peek-source-link").first()).toContainText(
    "Northwind Stays job board",
  );
  await expect(page.getByTestId("peek-source-link").first()).toHaveAttribute(
    "href",
    new RegExp(String.raw`^https://boards\.greenhouse\.io/northwind-${project}/jobs/`),
  );

  // Watch a company yourself: a page with JobPosting data is detected, one with no job board is not.
  await page.goto("/companies?tab=watching");
  await page.getByTestId("companies-add-url").fill(`http://contoso-${project}.test:9020/careers`);
  await page.getByTestId("companies-add-go").click();
  const contoso = page.getByTestId(/^watching-row-/).filter({ hasText: `contoso-${project}.test` });
  await expect(contoso.getByTestId(/^watching-board-/)).toContainText("JobPosting", {
    timeout: 30_000,
  });
  await expect(contoso.getByTestId(/^watching-open-/)).toContainText("2 open jobs");

  await page.getByTestId("companies-add-url").fill(`http://tailspin-${project}.test:9020/careers`);
  await page.getByTestId("companies-add-go").click();
  const tailspin = page
    .getByTestId(/^watching-row-/)
    .filter({ hasText: `tailspin-${project}.test` });
  await expect(tailspin.getByTestId(/^watching-nofound-/)).toContainText(
    "Couldn’t find their job board",
    { timeout: 30_000 },
  );
  await expect(tailspin.getByTestId(/^watching-careers-/)).toBeVisible();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // How it works and what it costs.
  await page.getByTestId("companies-tab-how").click();
  await expect(page.getByTestId("companies-how-cost")).toContainText("about €0.0003 each");
  await expect(page.getByTestId("companies-how-cost")).toContainText("Up to 20 watched companies");

  // Stop watching them all, so the stack's other searches read nothing of ours.
  await page.getByTestId("companies-tab-watching").click();
  for (const host of [northwind, `contoso-${project}.test`, `tailspin-${project}.test`]) {
    await page
      .getByTestId(/^watching-row-/)
      .filter({ hasText: host })
      .getByTestId(/^watching-stop-/)
      .click();
    await expect(page.getByTestId(/^watching-row-/).filter({ hasText: host })).toHaveCount(0);
  }
});
