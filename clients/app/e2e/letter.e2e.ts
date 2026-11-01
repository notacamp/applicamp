import { expect, test, type Page } from "@playwright/test";

import {
  callApi,
  callApiJson,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

/** An invented profile (nobody real): one role with a highlight with numbers. */
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
            id: `e2e-letter-fact-${tag}`,
            text: "Moved invoicing onto PostgreSQL",
            quote: "Moved invoicing onto PostgreSQL",
            area: "results",
            kept_at: "2026-10-30T09:00:00Z",
          },
        ],
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

/** A posting on the fake company's own site (it lists two application questions). */
async function addJob(page: Page, tag: string) {
  const created = await callApiJson(page, "POST", "/api/v1/opportunities", {
    url: `http://example-co.test:9020/jobs/staff-engineer?run=${tag}&skill=Hotwire-${tag}`,
    title: "Staff Engineer",
    company: `Example Co ${tag}`,
  });
  expect(created.status).toBe(201);
  const id = (created.body as { id: string }).id;
  added.push(id);
  expect(
    (await callApiJson(page, "PATCH", `/api/v1/opportunities/${id}`, { stage: "shortlisted" }))
      .status,
  ).toBe(200);
  return id;
}

interface CvAnswer {
  draft: {
    headline?: { id: string; flags: unknown[] } | null;
    summary?: { id: string; flags: unknown[] } | null;
    experience: { bullets: { id: string; flags: unknown[] }[] }[];
    skills: { id: string; flags: unknown[] }[];
  };
}

/** Reads the company's pages, drafts the CV, removes what is flagged and approves it. */
async function approvedCv(page: Page, id: string) {
  await page.goto(`/pipeline/${id}/prepare`);
  await page.getByTestId("prepare-read").click();
  await expect(page.getByTestId("prepare-facts")).toContainText("Builds ledgers for small shops");
  const made = await callApiJson(page, "POST", `/api/v1/opportunities/${id}/cv/drafts`, {});
  expect(made.status).toBe(200);
  let cv = made.body as CvAnswer;
  for (;;) {
    const lines = [
      cv.draft.headline,
      cv.draft.summary,
      ...cv.draft.experience.flatMap((role) => role.bullets),
      ...cv.draft.skills,
    ];
    const open = lines.find((line) => line !== null && line !== undefined && line.flags.length > 0);
    if (open === undefined || open === null) {
      break;
    }
    const removed = await callApiJson(
      page,
      "PATCH",
      `/api/v1/opportunities/${id}/cv/drafts/1/lines/${open.id}`,
      { remove: true },
    );
    expect(removed.status).toBe(200);
    cv = removed.body as CvAnswer;
  }
  expect(
    (await callApiJson(page, "POST", `/api/v1/opportunities/${id}/cv/drafts/1/approve`, {})).status,
  ).toBe(200);
}

test("someone writes the cover letter after approving the CV, fixes what can't be traced and the question the profile can't answer, approves it and finds the PDF on the CVs page", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  const tag = `${testInfo.project.name}-${String(testInfo.retry)}-${Date.now().toString(36)}`;
  expect(await callApi(page, "PUT", "/api/v1/profile", profile(tag))).toBe(200);
  const id = await addJob(page, tag);

  // Until the CV is approved, the letter says why it can't be written.
  await page.goto(`/pipeline/${id}/letter`);
  await expect(page.getByTestId("letter-write")).toBeDisabled();
  await expect(page.getByTestId("letter-write-reason")).toContainText(
    "Approve your CV for this job first",
  );

  // Approved CV → "Write the cover letter" on its review page.
  await approvedCv(page, id);
  await page.goto(`/pipeline/${id}/cv`);
  await expect(page.getByTestId("cv-letter-write")).toContainText(
    "Write the cover letter · about €0.002",
  );
  await page.getByTestId("cv-letter-write").click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/letter$`));
  await expectPageFillsTheScreen(page, "letter-review");

  // The letter as a page: the server's greeting and contact, each paragraph tagged.
  await expect(page.getByTestId("letter-paper")).toBeVisible();
  await expect(page.getByTestId("letter-name")).toContainText("Alex Demo");
  await expect(page.getByTestId("letter-contact")).toContainText("alex.demo@example.com");
  await expect(page.getByTestId("letter-greeting")).toContainText(
    `Dear hiring team at Example Co ${tag},`,
  );
  await expect(page.getByTestId("letter-status")).toContainText("Draft");
  const paragraphs = page.getByTestId(/^letter-paragraph-p\d+$/);
  const opening = paragraphs.filter({ hasText: "Builds ledgers for small shops" });
  await expect(opening.getByTestId(/^letter-tags-/)).toContainText("Company fact");
  await expect(opening.getByTestId(/^letter-tags-/)).toContainText("From your profile");
  await expect(
    paragraphs.filter({ hasText: "bring that experience" }).getByTestId(/^letter-tags-/),
  ).toContainText("No claims");
  await opening.getByTestId(/^letter-from-toggle-/).click();
  await expect(opening).toContainText("Rebuilt the billing pipeline in Rust");

  // Flagged, never dropped: the praise, the company-only claim and the question.
  await expect(page.getByTestId("letter-banner-flagged")).toContainText("3 things need you");
  await expect(page.getByTestId("letter-approve")).toBeDisabled();
  await expect(page.getByTestId("letter-glance")).toContainText("matches CV v1");
  await expect(page.getByTestId("letter-drafts-today")).toContainText("of 20");
  await expect(page.getByTestId("letter-month-spend")).toBeVisible();
  const praise = paragraphs.filter({ hasText: "go-to person" });
  await expect(praise.getByTestId(/^letter-flag-/)).toContainText("“go-to” isn’t in your profile");
  const claim = paragraphs.filter({ hasText: "I led our migration" });
  await expect(claim.getByTestId(/^letter-flag-/)).toContainText(
    "cites only a fact about the company",
  );
  const notice = page.getByTestId(/^letter-answer-q\d+$/).filter({ hasText: "notice period" });
  await expect(notice.getByTestId(/^letter-answer-flag-/)).toContainText(
    "Your profile and goals don’t say",
  );
  const why = page.getByTestId(/^letter-answer-q\d+$/).filter({ hasText: "Why do you want" });
  await expect(why.getByTestId(/^letter-answer-text-/)).toContainText(
    "matches the work I have done",
  );
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // "Go to the first one" lands on a flagged sentence.
  await page.getByTestId("letter-first-flagged").click();
  await expect(praise.getByTestId(/^letter-flag-/)).toBeVisible();

  // It's true: the person's words become a fact on the profile.
  await praise.getByTestId(/^letter-confirm-s\d+$/).click();
  await praise
    .getByTestId(/^letter-confirm-input-/)
    .fill("Known as the go-to person for the hardest billing problems");
  await praise.getByTestId(/^letter-confirm-save-/).click();
  await expect(page.getByTestId("letter-banner-flagged")).toContainText("2 things need you");
  await expect(praise.getByTestId(/^letter-flag-/)).toHaveCount(0);

  // Edit: said about the company, it passes.
  await claim.getByTestId(/^letter-edit-s\d+$/).click();
  await claim.getByTestId(/^letter-edit-input-/).fill("Example Co builds ledgers for small shops.");
  await claim.getByTestId(/^letter-edit-save-/).click();
  await expect(page.getByTestId("letter-banner-flagged")).toContainText("1 thing needs you");

  // The question the profile can't answer: the person's own words.
  await notice.getByTestId(/^letter-answer-input-/).fill("One month.");
  await notice.getByTestId(/^letter-answer-save-/).click();
  await expect(notice.getByTestId(/^letter-answer-text-/)).toContainText("One month.");
  await expect(notice.getByTestId(/^letter-answer-own-/)).toContainText("Your answer");
  await expect(page.getByTestId("letter-banner-clean")).toContainText("Every line traces back");
  await expectNoSidewaysScroll(page);

  // Approve: one page, read back, kept as v1; the answers are not in it.
  await expect(page.getByTestId("letter-approve")).toBeEnabled();
  await expectAccessible(page);
  await page.getByTestId("letter-approve").click();
  await expect(page.getByTestId("letter-status")).toContainText("Approved · v1");
  await expect(page.getByTestId("letter-banner-approved")).toBeVisible();
  await expect(page.getByTestId("letter-approve")).toHaveCount(0);
  await expect(page.getByTestId("letter-pdf")).toContainText("1 page");
  await page.getByTestId("letter-open-pdf").click();
  await expect(page.getByTestId("pdf-viewer")).toBeVisible();
  await expect(page.getByTestId("pdf-viewer-page-1")).toHaveAttribute("data-drawn", "");
  await expect(page.getByTestId("pdf-viewer-download")).toHaveAttribute(
    "href",
    `/api/v1/opportunities/${id}/letter/versions/1/pdf?download=1`,
  );
  await expectNoSidewaysScroll(page);
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("pdf-viewer")).toHaveCount(0);

  // The answer is text with Copy.
  await expect(why.getByTestId(/^letter-answer-copy-/)).toBeVisible();

  // The CVs page lists the letter under Letters, next to the CV under CVs.
  await page.goto("/cvs");
  await page.getByTestId("cvs-filter-letters").click();
  const mine = page.getByTestId("cvs-row").filter({ hasText: `Example Co ${tag}` });
  await expect(mine).toHaveCount(1);
  await expect(mine.getByTestId("cvs-row-document")).toContainText("Cover letter");
  await expect(mine.getByTestId("cvs-row-version")).toContainText("v1");
  await mine.getByTestId("cvs-view").click();
  await expect(page.getByTestId("pdf-viewer-page-1")).toHaveAttribute("data-drawn", "");
  await page.getByTestId("pdf-viewer-close").click();
  await page.getByTestId("cvs-filter-cvs").click();
  await expect(
    page
      .getByTestId("cvs-row")
      .filter({ hasText: `Example Co ${tag}` })
      .getByTestId("cvs-row-document"),
  ).toContainText("CV");
  await expectNoSidewaysScroll(page);

  // The peek's Tailored CV block has the Cover letter line.
  await page.goto("/pipeline");
  await page.getByTestId("pipeline-search").fill(`Example Co ${tag}`);
  const row = page.getByTestId("pipeline-row").first();
  await expect(row).toBeVisible();
  await (phone ? row.click() : row.getByTestId("pipeline-row-open").click());
  await expect(page.getByTestId("peek-letter-state")).toContainText("v1 approved");
  await expect(page.getByTestId("peek-letter-view-pdf")).toBeVisible();
  await page.getByTestId("peek-letter-review").click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/letter$`));

  // The person's words are a fact on the profile.
  await page.goto("/profile");
  await expect(page.getByTestId("role-facts-list-0")).toContainText(
    "Known as the go-to person for the hardest billing problems",
  );
  await expectNoSidewaysScroll(page);
});
