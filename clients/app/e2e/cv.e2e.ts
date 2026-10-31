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
 * An invented profile (nobody real): three roles, a headline, a highlight
 * with numbers, and a fact the person kept, so the fake model has a
 * reworded line, a word-for-word line and a left-out role to work with.
 */
function profile(tag: string) {
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
        facts: [
          {
            id: `e2e-fact-${tag}`,
            text: "Moved invoicing onto PostgreSQL",
            quote: "Moved invoicing onto PostgreSQL",
            area: "results",
            kept_at: "2026-10-30T09:00:00Z",
          },
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
      {
        name: "Tiny Co",
        position: "Junior Developer",
        startDate: "2015",
        endDate: "2017",
        highlights: ["Wrote internal tools"],
      },
    ],
    education: [{ institution: "Example University", studyType: "BSc", area: "Computer science" }],
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
});

/** A posting on the fake company's own site, shortlisted when `stage` says. */
async function addJob(page: Page, tag: string, stage: string) {
  const created = await callApiJson(page, "POST", "/api/v1/opportunities", {
    url: `http://example-co.test:9020/jobs/staff-engineer?run=${tag}&skill=Hotwire-${tag}`,
    title: "Staff Engineer",
    company: `Example Co ${tag}`,
  });
  expect(created.status).toBe(201);
  const id = (created.body as { id: string }).id;
  added.push(id);
  if (stage !== "found") {
    expect(
      (await callApiJson(page, "PATCH", `/api/v1/opportunities/${id}`, { stage })).status,
    ).toBe(200);
  }
  return id;
}

test("someone drafts a CV, sees the lines that can't be traced to their profile, fixes one by editing, one in their own words and one by removing a skill, and approves it", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  const tag = `${testInfo.project.name}-${String(testInfo.retry)}-${Date.now().toString(36)}`;
  expect(await callApi(page, "PUT", "/api/v1/profile", profile(tag))).toBe(200);
  const id = await addJob(page, tag, "shortlisted");

  // Prepare: read the company's pages (the facts the CV may cite), then draft.
  await page.goto(`/pipeline/${id}/prepare`);
  await expect(page.getByTestId("prepare-draft")).toContainText("Draft the CV · about €0.002");
  await page.getByTestId("prepare-read").click();
  await expect(page.getByTestId("prepare-facts")).toContainText("Builds ledgers for small shops");
  await page.getByTestId("prepare-draft").click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/cv$`));
  await expectPageFillsTheScreen(page, "cv-review");

  // The CV as a page, the contact filled in by the server.
  await expect(page.getByTestId("cv-paper")).toBeVisible();
  await expect(page.getByTestId("cv-name")).toContainText("Alex Demo");
  await expect(page.getByTestId("cv-contact")).toContainText("alex.demo@example.com");
  await expect(page.getByTestId("cv-draft-number")).toContainText("Draft 1");
  await expect(page.getByTestId("cv-status")).toContainText("Draft");
  await expect(page.getByTestId("cv-education")).toContainText("Example University");
  await expect(page.getByTestId("cv-languages")).toContainText("English (Fluent)");

  // Each line says how it relates to the profile.
  const lines = page.getByTestId(/^cv-line-l\d+$/);
  const reworded = lines.filter({ hasText: "Delivered: Moved invoicing onto PostgreSQL" });
  await expect(reworded.getByTestId(/^cv-line-mark-/)).toContainText("Reworded");
  await reworded.getByTestId(/^cv-from-toggle-/).click();
  await expect(reworded.getByTestId(/^cv-from-l\d+$/)).toContainText(
    "Moved invoicing onto PostgreSQL",
  );
  // A highlight copied word for word (the summary quotes it too, and a flagged line holds its source: so look in
  // Experience, at the lines that start with it).
  const verbatim = page
    .getByTestId("cv-experience")
    .getByTestId(/^cv-line-l\d+$/)
    .filter({
      hasText: /^Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours/,
    });
  await expect(verbatim.getByTestId(/^cv-line-mark-/)).toContainText("As in profile");

  // Three things can't be traced: Approve is off and the aside says so first.
  await expect(page.getByTestId("cv-banner-flagged")).toContainText(
    "3 lines can’t be traced to your profile",
  );
  await expect(page.getByTestId("cv-approve")).toBeDisabled();
  await expect(page.getByTestId("cv-approve")).toContainText("Approve v1");
  const banner = await page.getByTestId("cv-first-flagged").boundingBox();
  const paper = await page.getByTestId("cv-paper").boundingBox();
  expect(banner).not.toBeNull();
  expect(paper).not.toBeNull();
  if (banner !== null && paper !== null) {
    if (phone) {
      // One column, the aside above the CV.
      expect(banner.y + banner.height).toBeLessThanOrEqual(paper.y + 1);
    } else {
      // Two columns, the aside beside the CV.
      expect(banner.x).toBeGreaterThanOrEqual(paper.x + paper.width - 1);
    }
  }
  await expect(page.getByTestId("cv-keywords-count")).toContainText("/");
  await expect(page.getByTestId("cv-changes")).toContainText("left out");
  await expect(page.getByTestId("cv-drafts-today")).toContainText("of 20");
  await expect(page.getByTestId("cv-month-spend")).toBeVisible();
  await expect(page.getByTestId("cv-left-out")).toContainText("Left out for this job · 1 item");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // "Go to the first one" lands on a flagged line.
  await page.getByTestId("cv-first-flagged").click();
  const flagged = lines.filter({ has: page.getByTestId(/^cv-flag-/) });
  await expect(flagged).toHaveCount(2);

  // A number nobody wrote: the person says it in their own words, and it becomes a fact.
  const number = lines.filter({ hasText: "from 23 hours to 40 minutes" });
  await expect(number.getByTestId(/^cv-flag-/)).toContainText("“23” isn’t in the sources");
  await number.getByTestId(/^cv-confirm-l/).click();
  await number
    .getByTestId(/^cv-confirm-input-/)
    .fill("Invoice runs took 23 hours before I rebuilt the pipeline");
  await number.getByTestId(/^cv-confirm-save-/).click();
  await expect(page.getByTestId("cv-banner-flagged")).toContainText(
    "2 lines can’t be traced to your profile",
  );
  await expect(number.getByTestId(/^cv-flag-/)).toHaveCount(0);

  // A company fact used as an achievement: edited back to what the profile says.
  const company = lines.filter({ hasText: "for customers like those of" });
  await expect(company.getByTestId(/^cv-flag-/)).toContainText(
    "Company facts can’t be your achievements",
  );
  await expect(company.getByTestId(/^cv-confirm-l/)).toHaveCount(0);
  await company.getByTestId(/^cv-edit-l/).click();
  await company.getByTestId(/^cv-edit-input-/).fill("Led a team of 4 engineers");
  await company.getByTestId(/^cv-edit-save-/).click();
  await expect(page.getByTestId("cv-banner-flagged")).toContainText(
    "1 line can’t be traced to your profile",
  );

  // A skill that is not on the profile: removed.
  const skill = page.getByTestId(/^cv-skill-flag-/);
  await expect(skill).toContainText("Kubernetes isn’t one of your skills");
  await expectNoSidewaysScroll(page);
  await skill.getByTestId(/^cv-skill-remove-/).click();
  await expect(page.getByTestId("cv-banner-clean")).toContainText(
    "Every line traces to your profile",
  );
  await expect(page.getByTestId("cv-approve-reason")).toHaveCount(0);

  // Approve: a version that can't change.
  await expect(page.getByTestId("cv-approve")).toBeEnabled();
  await expectAccessible(page);
  await page.getByTestId("cv-approve").click();
  await expect(page.getByTestId("cv-status")).toContainText("Approved · v1");
  await expect(page.getByTestId("cv-banner-approved")).toBeVisible();
  await expect(page.getByTestId("cv-approve")).toHaveCount(0);
  await expectNoSidewaysScroll(page);

  // The PDF: read back, opened in the viewer (a page drawn on a canvas), downloadable.
  await expect(page.getByTestId("cv-pdf-checks")).toContainText("Read back from the PDF");
  await expect(page.getByTestId("cv-pdf-check-contact")).toBeVisible();
  await page.getByTestId("cv-open-pdf").click();
  await expect(page.getByTestId("pdf-viewer")).toBeVisible();
  await expect(page.getByTestId("pdf-viewer-page-1")).toHaveAttribute("data-drawn", "");
  await expect(page.getByTestId("pdf-viewer-download")).toHaveAttribute(
    "href",
    `/api/v1/opportunities/${id}/cv/versions/1/pdf?download=1`,
  );
  await expectNoSidewaysScroll(page);
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("pdf-viewer")).toHaveCount(0);

  // Compact: a new version, no AI; v1 stays.
  await page.getByTestId("cv-template-compact").click();
  await page.getByTestId("cv-layout-confirm").click();
  await expect(page.getByTestId("cv-version-2")).toBeVisible();
  await expect(page.getByTestId("cv-version-1")).toBeVisible();
  await expectNoSidewaysScroll(page);

  // The CVs page lists both and opens a PDF.
  await page.goto("/cvs");
  await expect(page.getByTestId("cvs")).toBeVisible();
  await page.getByTestId("cvs-filter-approved").click();
  const mine = page.getByTestId("cvs-row").filter({ hasText: `Example Co ${tag}` });
  await expect(mine).toHaveCount(2);
  await expect(mine.first().getByTestId("cvs-row-version")).toContainText("v2");
  await mine.nth(1).getByTestId("cvs-view").click();
  await expect(page.getByTestId("pdf-viewer-page-1")).toHaveAttribute("data-drawn", "");
  await page.getByTestId("pdf-viewer-close").click();
  await expectNoSidewaysScroll(page);
  await page.goBack();

  // The peek shows the newest version: v2, the Compact one.
  await page.goto("/pipeline");
  await page.getByTestId("pipeline-search").fill(`Example Co ${tag}`);
  const row = page.getByTestId("pipeline-row").first();
  await expect(row).toBeVisible();
  await (phone ? row.click() : row.getByTestId("pipeline-row-open").click());
  await expect(page.getByTestId("peek-cv-state")).toContainText("v2 approved");
  await page.getByTestId("peek-review").click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/cv$`));

  // The person's words are a fact on the profile, under the role the line was about.
  await page.goto("/profile");
  await expect(page.getByTestId("role-facts-list-0")).toContainText(
    "Invoice runs took 23 hours before I rebuilt the pipeline",
  );
  await expectNoSidewaysScroll(page);
});

test("Draft the CV is off, with its reason, until the job is shortlisted", async ({
  page,
}, testInfo) => {
  await signIn(page);
  const tag = `${testInfo.project.name}-${String(testInfo.retry)}-${Date.now().toString(36)}`;
  const id = await addJob(page, tag, "found");
  await page.goto(`/pipeline/${id}/prepare`);
  await expect(page.getByTestId("prepare-draft")).toBeDisabled();
  await expect(page.getByTestId("prepare-draft-reason")).toContainText("Shortlist this job first");
  await expectNoSidewaysScroll(page);
});
