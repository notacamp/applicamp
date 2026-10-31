import { expect, test, type Page } from "@playwright/test";

import {
  callApi,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

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
