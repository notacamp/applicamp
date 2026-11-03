import { expect, test } from "@playwright/test";

import {
  callApi,
  callApiJson,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  setAutonomy,
  signIn,
} from "./helpers";

// Filmed: CI keeps this spec's videos (the `e2e-recordings` artifact), the
// recording of preparing a job by itself at desktop width and at 375px.
test.use({ video: "on" });

/** An invented profile (nobody real): two roles with highlights to draft from. */
function profile() {
  return {
    basics: {
      name: "Alex Demo",
      label: "Backend engineer who ships billing systems",
      email: "alex.demo@example.com",
      location: "Porto, Portugal",
      profiles: [],
    },
    work: [
      {
        name: "Example Org",
        position: "Senior Engineer",
        startDate: "2021-03",
        summary: "Runs the billing platform.",
        highlights: [
          "Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes",
          "Led a team of 4 engineers",
        ],
      },
      {
        name: "Sample Labs",
        position: "Engineer",
        startDate: "2017-06",
        endDate: "2021-02",
        summary: "Built the data pipeline.",
        highlights: ["Built ingestion for 3 partner feeds"],
      },
    ],
    education: [],
    skills: [{ name: "Rust" }, { name: "PostgreSQL" }],
    languages: [{ language: "English", fluency: "Fluent" }],
    certificates: [],
    projects: [],
  };
}

/** The opportunities a test added: put away at the end ("Not for me"). */
const added: string[] = [];

test.afterEach(async ({ page }) => {
  for (const id of added.splice(0)) {
    await callApiJson(page, "PATCH", `/api/v1/opportunities/${id}`, { dismissed: true });
  }
  // The other specs click every step themselves.
  await setAutonomy(page, "find_only");
});

/** The background job and the page's polling: a few seconds per step. */
const WORK = { timeout: 60_000 };

test("someone shortlists a job and the app gets it ready by itself: it reads the company, waits on the questions only they can answer, drafts the CV for their review, and Today says what waits on them", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  await setAutonomy(page, "prepare");
  expect(await callApi(page, "PUT", "/api/v1/profile", profile())).toBe(200);

  // A posting on the company's own site (the fake): its host is the company's site.
  const tag = `${testInfo.project.name}-${String(testInfo.retry)}-${Date.now().toString(36)}`;
  const company = `Example Co ${tag}`;
  const created = await callApiJson(page, "POST", "/api/v1/opportunities", {
    url: `http://example-co.test:9020/jobs/staff-engineer?run=${tag}&skill=Hotwire-${tag}`,
    title: "Staff Engineer",
    company,
  });
  expect(created.status).toBe(201);
  const id = (created.body as { id: string }).id;
  added.push(id);

  // The person's one click: Shortlisted, in the job's peek.
  await page.goto("/pipeline");
  await page.getByTestId("pipeline-search").fill(company);
  const row = page.getByTestId("pipeline-row").first();
  await expect(row).toBeVisible();
  await (phone ? row.click() : row.getByTestId("pipeline-row-open").click());
  await page.getByTestId("peek-stage").selectOption("shortlisted");
  await expect(page.getByTestId("peek-stage")).toHaveValue("shortlisted");
  await page.getByTestId("peek-prepare").click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/prepare$`));
  await expectPageFillsTheScreen(page, "prepare");

  // By itself: the company read, the job compared; then it waits on the person.
  await expect(page.getByTestId("prepare-state-waiting")).toContainText(
    "Waiting for your answers",
    WORK,
  );
  await expect(page.getByTestId("prepare-site")).toContainText("example-co.test");
  await expect(page.getByTestId("prepare-facts")).toContainText("Builds ledgers for small shops");
  await expect(page.getByTestId("prepare-done")).toContainText("read the company");
  await expect(page.getByTestId("prepare-done")).toContainText("compared the job");
  await expect(page.getByTestId("prepare-want-0")).toContainText("Rails");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // The person skips the questions: the CV is drafted by itself, and waits for the review.
  const questions = page.getByTestId(/^prepare-question-[0-9a-f-]{36}$/);
  await expect(questions.first()).toBeVisible();
  for (let left = await questions.count(); left > 0; left -= 1) {
    await questions
      .first()
      .getByTestId(/^prepare-skip-/)
      .click();
    await expect(questions).toHaveCount(left - 1);
  }
  await expect(page.getByTestId("prepare-state-waiting")).toContainText(
    "Your CV for this job is drafted",
    WORK,
  );
  await expect(page.getByTestId("prepare-done")).toContainText("drafted the CV");
  await expectNoSidewaysScroll(page);

  // Approving stays theirs: Review opens the draft.
  await page.getByTestId("prepare-review").click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/cv$`));
  await expect(page.getByTestId("cv-paper")).toBeVisible();
  await expect(page.getByTestId("cv-status")).toContainText("Draft");

  // Today lists it with what it waits on, and opens its Prepare page.
  await page.goto("/");
  const waiting = page.getByTestId(`today-waiting-${id}`);
  await expect(waiting).toContainText(`Staff Engineer · ${company}: review your CV`);
  await expectNoSidewaysScroll(page);
  await page.getByTestId(`today-waiting-open-${id}`).click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/prepare$`));

  // Stop: nothing more runs by itself; "Prepare by itself" starts it again.
  await page.getByTestId("prepare-stop").click();
  await expect(page.getByTestId("prepare-state-stopped")).toContainText(
    "You stopped preparing this job.",
  );
  await expect(page.getByTestId("prepare-start")).toBeVisible();
});
