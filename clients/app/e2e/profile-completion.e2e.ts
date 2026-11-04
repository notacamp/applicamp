import { expect, test, type Page } from "@playwright/test";

import {
  callApi,
  callApiJson,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  pdfSaying,
  signIn,
} from "./helpers";

// Filmed: CI keeps this spec's videos (the `e2e-recordings` artifact), the
// recording of the profile's strength and of a LinkedIn PDF read again, at
// desktop width and at 375px.
test.use({ video: "on" });

/**
 * An invented profile (nobody real) with the gaps the strength shows: three
 * roles without results, no headline, summary, education or link.
 */
const PROFILE = {
  basics: { profiles: [] },
  work: [
    {
      name: "Example Org",
      position: "Senior Engineer",
      startDate: "2021-03",
      summary: "Runs the billing platform in Rust, with Kafka between its services.",
      highlights: [],
    },
    {
      name: "Sample Labs",
      position: "Engineer",
      startDate: "2017-06",
      endDate: "2021-02",
      summary: "Built the data pipeline in Rust.",
      highlights: [],
    },
    {
      name: "Third Co",
      position: "Developer",
      startDate: "2014-01",
      endDate: "2017-05",
      summary: "Wrote internal tools.",
      highlights: [],
    },
  ],
  education: [],
  skills: [{ name: "PostgreSQL" }, { name: "Rust" }, { name: "Kafka" }],
  languages: [],
  certificates: [],
  projects: [],
};

/** The percentage the strength card shows. */
async function scoreOf(page: Page): Promise<number> {
  const text = (await page.getByTestId("strength-score").textContent()) ?? "";
  const match = /(\d+)%/.exec(text);
  return Number(match?.[1] ?? "-1");
}

test("someone sees how strong their profile is, is led to Tell me about you from the card, a part and Today, and puts a skill first on the profile", async ({
  page,
}) => {
  await signIn(page);
  // Both projects share one stack and one person, in any order: start from the same profile.
  expect(await callApi(page, "PUT", "/api/v1/profile", PROFILE)).toBe(200);

  // Profile & goals: the strength, what each part needs, and no Improve or Grill me.
  await page.goto("/profile");
  await expectPageFillsTheScreen(page, "profile");
  await expect(page.getByTestId("strength-card")).toBeVisible();
  expect(await scoreOf(page)).toBeLessThan(80);
  await expect(page.getByTestId("strength-headline-action")).toBeVisible();
  await expect(page.getByTestId("strength-improve")).toHaveCount(0);
  await expect(page.getByTestId("strength-grill")).toHaveCount(0);
  await expect(page.getByTestId("role-about-0")).toBeVisible();
  await expect(page.getByTestId("role-grill-0")).toHaveCount(0);
  await expect(page.getByTestId("role-facts")).toContainText(
    "What you told Applicamp about each role",
  );
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // The strength card and a part that needs more than a field both lead to the wizard.
  await page.getByTestId("strength-about").click();
  await expect(page).toHaveURL(/\/profile\/about$/);
  await page.goto("/profile");
  await page.getByTestId("strength-achievements-action").click();
  await expect(page).toHaveURL(/\/profile\/about$/);

  // Today puts the strength first while it isn't strong; Continue leads to the
  // wizard, and "Not now" puts the card away.
  await page.goto("/");
  await expect(page.getByTestId("today-profile-nudge")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await page.getByTestId("nudge-continue").click();
  await expect(page).toHaveURL(/\/profile\/about$/);
  await page.goto("/");
  await page.getByTestId("nudge-not-now").click();
  await expect(page.getByTestId("today-profile-nudge")).toHaveCount(0);
  await page.reload();
  await expect(page.getByTestId("today")).toBeVisible();
  await expect(page.getByTestId("today-profile-nudge")).toHaveCount(0);

  // Skills, in the order they count: the first three drive the searches.
  await page.goto("/profile");
  await page.getByTestId("section-skills-edit").click();
  const rows = page.getByTestId("section-skills-row");
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(2)).toContainText("Kafka");
  await expect(page.getByTestId("section-skills-top-0")).toBeVisible();
  await page.getByTestId("section-skills-up-2").click();
  await expect(rows.nth(1)).toContainText("Kafka");
  await page.getByTestId("section-skills-up-1").click();
  await expect(rows.nth(0)).toContainText("Kafka");
  await expect(page.getByTestId("section-skills-up-0")).toBeDisabled();
  await page.reload();
  await page.getByTestId("section-skills-edit").click();
  await expect(rows.nth(0)).toContainText("Kafka");
  await expectNoSidewaysScroll(page);
});

/**
 * An invented LinkedIn profile saved as PDF (nobody real), in the order its
 * text comes out: the sidebar, then the name, the About text, each position
 * with its dates line, its place line and its bullets, and education. The
 * fake model reads it wrong the known ways (#72): the first position's place
 * as its summary, and no education or languages until it is asked again.
 */
const LINKEDIN_PDF = [
  "Contact",
  "www.example.com/in/alex-demo",
  "Top Skills",
  "Rust",
  "PostgreSQL",
  "Team leadership",
  "Languages",
  "Portuguese (Native or Bilingual)",
  "English (Full Professional)",
  "Alex Demo",
  "Staff engineer building calm backends",
  "Lisbon, Portugal",
  "About",
  "I build boring, reliable backends for small teams.",
  "Experience",
  "Example Org",
  "Staff Engineer",
  "Jul 2023 - Present (1 year 4 months)",
  "Lisbon, Portugal",
  "- Moved billing to Rust",
  "- Cut refunds by half",
  "Sample Labs",
  "Engineer",
  "Jun 2017 - Apr 2023 (5 years 11 months)",
  "Porto, Portugal",
  "Built the data pipeline for 40 shops.",
  "- Ran the on-call rota",
  "Education",
  "Sample University",
  "Master's degree, Computer science",
  "2012 - 2017",
];

/** The profile an earlier import left: each position's place as its summary, no end date. */
const PLACES_AS_SUMMARIES = {
  basics: { name: "Alex Demo", profiles: [] },
  work: [
    {
      name: "Example Org",
      position: "Staff Engineer",
      startDate: "2023-07",
      summary: "Lisbon, Portugal",
      highlights: [],
    },
    {
      name: "Sample Labs",
      position: "Engineer",
      startDate: "2017-06",
      summary: "Porto, Portugal",
      highlights: [],
    },
  ],
  education: [],
  skills: [{ name: "Rust" }],
  languages: [],
  certificates: [],
  projects: [],
};

interface WorkAnswer {
  summary?: string;
  location?: string;
  endDate?: string;
  highlights: string[];
}

test("someone reads their LinkedIn PDF again: every section is found, no place is a summary, and Merge repairs the positions", async ({
  page,
}) => {
  await signIn(page);
  expect(await callApi(page, "PUT", "/api/v1/profile", PLACES_AS_SUMMARIES)).toBe(200);

  // The PDF is picked where the LinkedIn export is offered: it is read as a PDF.
  await page.goto("/profile?import=new");
  await expectPageFillsTheScreen(page, "profile");
  await page.getByTestId("import-file").setInputFiles({
    name: "Profile.pdf",
    mimeType: "application/pdf",
    buffer: pdfSaying(LINKEDIN_PDF),
  });
  await page.getByTestId("import-submit").click();

  // What was read, part by part, and what is not in the file.
  await expect(page.getByTestId("import-review")).toBeVisible();
  await expect(page.getByTestId("review-found")).toContainText(
    "Found 2 jobs, 3 achievements, 3 skills, 1 degree, and 2 languages.",
  );
  await expect(page.getByTestId("review-empty-certificates")).toBeVisible();
  await expect(page.getByTestId("review-empty-projects")).toBeVisible();
  await expect(page.getByTestId("review-empty-education")).toHaveCount(0);
  // The place the model gave as a summary is the position's location.
  await expect(page.getByTestId("review-work")).toContainText("Lisbon, Portugal");
  await expect(page.getByTestId("review-education")).toContainText("Sample University");
  await expect(page.getByTestId("review-languages")).toContainText("English");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Merge repairs each position: the places leave the summaries, the end
  // date and the achievements are added, and nothing else changes.
  await page.getByTestId("review-merge").click();
  const work = page.getByTestId("section-work");
  await expect(work).toContainText("Lisbon, Portugal");
  await expect(work).toContainText("Porto, Portugal");
  const saved = await callApiJson(page, "GET", "/api/v1/profile");
  const positions = (saved.body as { profile: { work: WorkAnswer[] } }).profile.work;
  expect(positions).toHaveLength(2);
  expect(positions[0]?.summary).toBeUndefined();
  expect(positions[0]?.location).toBe("Lisbon, Portugal");
  expect(positions[0]?.highlights).toEqual(["Moved billing to Rust", "Cut refunds by half"]);
  expect(positions[1]?.summary).toBe("Built the data pipeline for 40 shops.");
  expect(positions[1]?.endDate).toBe("2023-04");
  await expectNoSidewaysScroll(page);
});
