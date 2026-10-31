import { expect, test, type Page } from "@playwright/test";

import {
  callApi,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

/**
 * An invented profile (nobody real) with the gaps the review finds: three
 * roles without results, no headline, summary, education or link, and
 * skills its roles mention that its list lacks.
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
  skills: [{ name: "PostgreSQL" }],
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

test("someone sees how strong their profile is, answers a question, keeps the draft, adds a skill found in their roles and puts it first", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  // Both projects share one stack and one person, in any order: start from the same profile.
  expect(await callApi(page, "PUT", "/api/v1/profile", PROFILE)).toBe(200);

  // Profile & goals: the strength, and what each part needs.
  await page.goto("/profile");
  await expectPageFillsTheScreen(page, "profile");
  await expect(page.getByTestId("strength-card")).toBeVisible();
  const before = await scoreOf(page);
  expect(before).toBeLessThan(80);
  await expect(page.getByTestId("strength-headline-action")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Today puts the strength first while it isn't strong, and "Not now" puts it away.
  await page.goto("/");
  await expect(page.getByTestId("today-profile-nudge")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await page.getByTestId("nudge-not-now").click();
  await expect(page.getByTestId("today-profile-nudge")).toHaveCount(0);
  await page.reload();
  await expect(page.getByTestId("today")).toBeVisible();
  await expect(page.getByTestId("today-profile-nudge")).toHaveCount(0);

  // Improve my profile: the AI read the profile and asks about the first gap.
  // (Profile & goals now leads to "Tell me about you"; the questions stay a page of their own.)
  await page.goto("/profile/improve");
  await expect(page).toHaveURL(/\/profile\/improve$/);
  await expect(page.getByTestId("improve-question")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId("improve-context")).toContainText("Example Org");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // The answer becomes a draft from the person's own words; Enter drafts, Shift+Enter is a new line.
  const answer = page.getByTestId("improve-answer");
  await answer.fill("Rebuilt billing in Rust.");
  await answer.press("Shift+Enter");
  await answer.pressSequentially("Invoice runs went from 6 hours to 40 minutes.");
  await expect(answer).toHaveValue(/Rust\.\nInvoice/);
  await answer.press("Enter");
  await expect(page.getByTestId("improve-draft-block")).toBeVisible();
  await expect(page.getByTestId("improve-draft-items")).toContainText("Rebuilt billing in Rust");
  await expect(page.getByTestId("improve-draft-items")).toContainText("6 hours to 40 minutes");
  await expectNoSidewaysScroll(page);

  // Nothing is saved until Keep; then the next question is asked.
  expect(
    await page.evaluate(async () => {
      const view = (await (await fetch("/api/v1/profile")).json()) as {
        profile: { work: { highlights: string[] }[] };
      };
      return view.profile.work[0]?.highlights.length ?? -1;
    }),
  ).toBe(0);
  await page.getByTestId("improve-keep").click();
  await expect(page.getByTestId("improve-kept")).toBeVisible();
  await expect(page.getByTestId("improve-answer")).toHaveValue("");

  // Esc finishes later: back to Today, with what was kept kept.
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("today")).toBeVisible();
  await page.goto("/profile");
  await expect(page.getByTestId("strength-card")).toBeVisible();
  await expect.poll(() => scoreOf(page)).toBeGreaterThan(before);

  // Skills found in your roles: Add puts it last, and the arrows put it first.
  await page.goto("/profile/skills");
  await expectPageFillsTheScreen(page, "profile-skills");
  await expect(page.getByTestId("skill-add-Rust")).toBeVisible({ timeout: 30_000 });
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  const kafkaIsThere = (await page.getByTestId("skill-dismiss-Kafka").count()) > 0;
  if (kafkaIsThere) {
    await page.getByTestId("skill-dismiss-Kafka").click();
    await expect(page.getByTestId("skill-add-Kafka")).toHaveCount(0);
  }
  await page.getByTestId("skill-add-Rust").click();
  await expect(page.getByTestId("skill-add-Rust")).toHaveCount(0);
  await expect(page.getByTestId("skill-rank-Rust")).toHaveText("2");
  await page.getByTestId("skill-up-Rust").click();
  await expect(page.getByTestId("skill-rank-Rust")).toHaveText("1");
  await expect(page.getByTestId("skill-up-Rust")).toBeDisabled();
  if (phone) {
    await expectNoSidewaysScroll(page);
  }
});
