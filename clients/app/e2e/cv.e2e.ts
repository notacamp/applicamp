import { expect, test, type Page } from "@playwright/test";

import {
  callApi,
  callApiJson,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  setAutonomy,
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

// Filmed: CI keeps this spec's videos (the `e2e-recordings` artifact), the
// recording of a CV made, shown as a PDF and approved at desktop width and at
// 375px.
test.use({ video: "on" });

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

test("someone drafts a CV, sees it as a PDF at once, says yes to the one question the app could not answer, removes a skill and approves it", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  await setAutonomy(page, "find_only");
  const tag = `${testInfo.project.name}-${String(testInfo.retry)}-${Date.now().toString(36)}`;
  expect(await callApi(page, "PUT", "/api/v1/profile", profile(tag))).toBe(200);
  const id = await addJob(page, tag, "shortlisted");

  // Prepare: read the company's pages (the facts the CV may cite), then draft.
  await page.goto(`/pipeline/${id}/prepare`);
  await expect(page.getByTestId("prepare-draft")).toContainText("Draft the CV");
  await expect(page.getByTestId("prepare-draft-cost")).toContainText(
    "Drafting the CV costs about €0.002.",
  );
  await page.getByTestId("prepare-read").click();
  await expect(page.getByTestId("prepare-facts")).toContainText("Builds ledgers for small shops");
  await page.getByTestId("prepare-draft").click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/cv$`));
  await expectPageFillsTheScreen(page, "cv-review");

  // The draft is a PDF at once, drawn on the page.
  await expect(page.getByTestId("cv-preview-pdf-page-1")).toHaveAttribute("data-drawn", "");
  await expect(page.getByTestId("cv-preview-facts")).toContainText("1 page · English");
  const preview = `/api/v1/opportunities/${id}/cv/drafts/1/preview.pdf`;
  const served = await page.request.get(preview);
  expect(served.status()).toBe(200);
  expect(served.headers()["content-type"]).toBe("application/pdf");
  expect(served.headers()["content-disposition"]).toContain("(draft).pdf");
  await expect(page.getByTestId("cv-preview-download")).toHaveAttribute(
    "href",
    `${preview}?download=1`,
  );
  await expect(page.getByTestId("cv-draft-number")).toContainText("Draft 1");
  await expect(page.getByTestId("cv-status")).toContainText("Draft");

  // The app checked every line and fixed one; two things wait on the person.
  await expect(page.getByTestId("cv-checked")).toContainText(
    "Checked every line against your profile · fixed 1 line",
  );
  await expect(page.getByTestId("cv-needs-you")).toContainText("Needs you · 2");
  const questions = page.getByTestId(/^cv-question-l\d+$/);
  const number = questions.filter({ hasText: "from 23 hours to 40 minutes" });
  await expect(number).toContainText("Is this true?");
  await expect(number).toContainText(
    "You told us about Example Org: “Rebuilt the billing pipeline in Rust, cutting invoice runs from 6 hours to 40 minutes”. If you keep it, it goes on your profile.",
  );
  const skill = questions.filter({ hasText: "Kubernetes" });
  await expect(skill).toContainText("Do you have this skill?");
  await expect(page.getByTestId("cv-approve")).toBeDisabled();
  await expect(page.getByTestId("cv-approve-reason")).toContainText(
    "Answer the 2 questions to approve.",
  );
  // The first question's Yes and the PDF's pages: boxes the page draws.
  const asks = await number.getByTestId(/^cv-question-yes-/).boundingBox();
  const pdf = await page.getByTestId("cv-preview-pdf-pages").boundingBox();
  expect(asks).not.toBeNull();
  expect(pdf).not.toBeNull();
  if (asks !== null && pdf !== null) {
    if (phone) {
      // One column: what needs the person comes first.
      expect(asks.y + asks.height).toBeLessThanOrEqual(pdf.y + 1);
    } else {
      // Two columns: the questions beside the PDF.
      expect(asks.x).toBeGreaterThanOrEqual(pdf.x + pdf.width - 1);
    }
  }
  // What the job wants, what the app did and what it rewrote, in plain words.
  await expect(page.getByTestId("cv-wants")).toContainText(
    "Kafka · nice to have · not on your profile",
  );
  await expect(page.getByTestId("cv-done")).toContainText("Wrote the CV");
  await expect(page.getByTestId("cv-done")).toContainText("Made the PDF");
  await expect(page.getByTestId("cv-fixed")).toContainText("Led a team of 4 engineers");
  await expect(page.getByTestId("cv-fixed")).toContainText("It said:");
  await expect(page.getByTestId("cv-layout-language")).toContainText("Style");
  await expect(page.getByTestId("cv-language-pt-PT")).toBeVisible();
  await expect(page.getByTestId("cv-keywords-count")).toContainText("/");
  await expect(page.getByTestId("cv-drafts-today")).toContainText("of 20");
  await expect(page.getByTestId("cv-month-spend")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Another layout: the PDF is made again, with no AI.
  const remade = page.waitForResponse(
    (response) =>
      response.url().endsWith(`/cv/drafts/1/preview`) && response.request().method() === "POST",
  );
  await page.getByTestId("cv-template-compact").click();
  expect((await remade).status()).toBe(200);
  await expect(page.getByTestId("cv-preview-pdf-page-1")).toHaveAttribute("data-drawn", "");
  const back = page.waitForResponse(
    (response) =>
      response.url().endsWith(`/cv/drafts/1/preview`) && response.request().method() === "POST",
  );
  await page.getByTestId("cv-template-clean").click();
  expect((await back).status()).toBe(200);

  // Yes, keep it: the fact goes on the profile and the line passes.
  await number.getByTestId(/^cv-question-yes-/).click();
  await expect(page.getByTestId("cv-needs-you")).toContainText("Needs you · 1");
  await expect(number).toHaveCount(0);

  // A skill the person does not have: removed.
  await skill.getByTestId(/^cv-question-remove-/).click();
  await expect(page.getByTestId("cv-needs-you")).toHaveCount(0);
  await expect(page.getByTestId("cv-approve-ready")).toContainText(
    "Approve when you’re happy with it.",
  );
  await expect(page.getByTestId("cv-preview-pdf-page-1")).toHaveAttribute("data-drawn", "");

  // Every line and where it comes from, one click away.
  await page.getByTestId("cv-lines-toggle").click();
  await expect(page.getByTestId("cv-paper")).toBeVisible();
  await expect(page.getByTestId("cv-name")).toContainText("Alex Demo");
  await expect(page.getByTestId("cv-contact")).toContainText("alex.demo@example.com");
  await expect(page.getByTestId("cv-education")).toContainText("Example University");
  // The position the CV leaves out is named under Earlier, as in the PDF.
  await expect(page.getByTestId("cv-earlier")).toContainText(
    "Junior Developer, Tiny Co (2015–2017)",
  );
  await expect(page.getByTestId("cv-left-out")).toContainText("Left out for this job · 1 item");
  await expectNoSidewaysScroll(page);

  // Approve: version 1, the PDF that was shown.
  await expect(page.getByTestId("cv-approve")).toBeEnabled();
  await expect(page.getByTestId("cv-approve")).toContainText("Approve v1");
  await expectAccessible(page);
  await page.getByTestId("cv-approve").click();
  await expect(page.getByTestId("cv-status")).toContainText("Approved · v1");
  await expect(page.getByTestId("cv-banner-approved")).toBeVisible();
  await expect(page.getByTestId("cv-approve")).toHaveCount(0);

  // For a moment it can be undone: a draft again, then approved again.
  await expect(page.getByTestId("cv-approved-now")).toContainText("Approved");
  await page.getByTestId("cv-undo").click();
  await expect(page.getByTestId("cv-status")).toContainText("Draft");
  await expect(page.getByTestId("cv-approve")).toBeEnabled();
  await page.getByTestId("cv-approve").click();
  await expect(page.getByTestId("cv-status")).toContainText("Approved · v1");
  await expectNoSidewaysScroll(page);

  // The PDF: read back, opened in the viewer, downloadable.
  await expect(page.getByTestId("cv-pdf-checks")).toContainText("Read back from the PDF");
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

  // The CVs page lists it and opens it.
  await page.goto("/cvs");
  await expect(page.getByTestId("cvs")).toBeVisible();
  await page.getByTestId("cvs-filter-cvs").click();
  const mine = page.getByTestId("cvs-row").filter({ hasText: `Example Co ${tag}` });
  await expect(mine).toHaveCount(1);
  await expect(mine.first().getByTestId("cvs-row-version")).toContainText("v1");

  // The peek shows it.
  await page.goto("/pipeline");
  await page.getByTestId("pipeline-search").fill(`Example Co ${tag}`);
  const row = page.getByTestId("pipeline-row").first();
  await expect(row).toBeVisible();
  await (phone ? row.click() : row.getByTestId("pipeline-row-open").click());
  await expect(page.getByTestId("peek-cv-state")).toContainText("v1 approved");

  // The yes is a fact on the profile, under the role the line was about.
  await page.goto("/profile");
  await expect(page.getByTestId("role-facts-list-0")).toContainText(
    "I rebuilt the billing pipeline in Rust, cutting invoice runs from 23 hours to 40 minutes.",
  );
  await expectNoSidewaysScroll(page);
});

test("Draft the CV is off, with its reason, until the job is shortlisted", async ({
  page,
}, testInfo) => {
  await signIn(page);
  await setAutonomy(page, "find_only");
  const tag = `${testInfo.project.name}-${String(testInfo.retry)}-${Date.now().toString(36)}`;
  const id = await addJob(page, tag, "found");
  await page.goto(`/pipeline/${id}/prepare`);
  await expect(page.getByTestId("prepare-draft")).toBeDisabled();
  await expect(page.getByTestId("prepare-draft-reason")).toContainText("Shortlist this job first");
  await expectNoSidewaysScroll(page);
});
