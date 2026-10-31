import { expect, test } from "@playwright/test";

import {
  callApi,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

/** An invented profile (nobody real) with two roles and nothing said about either. */
function profile(start: string) {
  return {
    basics: { profiles: [] },
    work: [
      {
        name: "Example Org",
        position: "Senior Engineer",
        startDate: start,
        summary: "Runs the billing platform.",
        highlights: [],
      },
      {
        name: "Sample Labs",
        position: "Engineer",
        startDate: "2017-06",
        endDate: "2021-02",
        summary: "Built the data pipeline.",
        highlights: [],
      },
    ],
    education: [],
    skills: [],
    languages: [],
    certificates: [],
    projects: [],
  };
}

test("someone is pushed back on for a vague answer, keeps, fixes and drops the facts taken from the next one, sees the coverage move and finds the kept facts on their profile", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  // The desktop and phone runs (and a retry) share one stack and one person:
  // a start date of their own makes each interview start from nothing.
  const month = (phone ? 6 : 3) + testInfo.retry;
  expect(
    await callApi(
      page,
      "PUT",
      "/api/v1/profile",
      profile(`2021-${String(month).padStart(2, "0")}`),
    ),
  ).toBe(200);

  // Profile & goals: the way into Grill me.
  await page.goto("/profile");
  await expect(page.getByTestId("strength-grill")).toBeVisible();
  await page.getByTestId("strength-grill").click();
  await expect(page).toHaveURL(/\/profile\/grill$/);
  await expectPageFillsTheScreen(page, "grill");
  await expect(page.getByTestId("grill-pick")).toBeVisible();
  await expect(page.getByTestId("grill-role-0")).toContainText("Senior Engineer");
  await expect(page.getByTestId("grill-role-0")).toContainText("not started");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // A role: nothing is asked until Start.
  await page.getByTestId("grill-role-0").click();
  await expect(page).toHaveURL(/\/profile\/grill\/0$/);
  await expect(page.getByTestId("grill-start")).toBeVisible();
  await expect(page.getByTestId("grill-coverage-count")).toHaveText("0/7");
  await expect(page.getByTestId("grill-cost")).toContainText("€0.001");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await page.getByTestId("grill-start").click();
  await expect(page.getByTestId("grill-turn-1")).toContainText("Example Org");

  // The coverage card is above the conversation on a phone, beside it on a desktop.
  const coverage = await page.getByTestId("grill-coverage").boundingBox();
  const chat = await page.getByTestId("grill-chat").boundingBox();
  expect(coverage).not.toBeNull();
  expect(chat).not.toBeNull();
  if (coverage !== null && chat !== null) {
    if (phone) {
      expect(coverage.y).toBeLessThan(chat.y);
    } else {
      expect(coverage.x).toBeGreaterThan(chat.x);
    }
  }

  // A vague answer is pushed back on, and nothing is proposed.
  const answer = page.getByTestId("grill-answer");
  await answer.fill("I did the rebuild, it went well.");
  await page.getByTestId("grill-send").click();
  await expect(page.getByTestId("grill-turn-3")).toContainText("Pushing back");
  await expect(page.getByTestId("grill-turn-3")).toContainText("Who decided");
  await expect(page.getByTestId(/^grill-facts-\d+$/)).toHaveCount(0);
  await expect(page.getByTestId("grill-coverage-count")).toHaveText("0/7");
  await expect(answer).toHaveValue("");

  // A real answer: Enter sends it, three facts are taken with the person's own words,
  // and the one the checks cannot trace to the answer is left out.
  await answer.fill("I proposed it and led it. 2 others helped part time. It took 7 months.");
  await answer.press("Enter");
  await expect(page.getByTestId("grill-facts-4")).toBeVisible();
  await expect(page.getByTestId("grill-left-out-4")).toContainText("1 line left out");
  const facts = page.getByTestId(/^grill-fact-/);
  await expect(facts).toHaveCount(3);
  await expect(page.getByTestId("grill-facts-4")).not.toContainText("30%");
  await expect(facts.first()).toContainText("Your words:");
  await expect(page.getByTestId("grill-coverage-count")).toHaveText("1/7");
  await expect(page.getByTestId("grill-area-context")).toHaveAttribute("data-state", "ok");
  await expect(page.getByTestId("grill-area-context")).toHaveAccessibleName("Covered");
  await expect(page.getByTestId("grill-area-your_part")).toHaveAttribute("data-state", "none");
  await expect(page.getByTestId("grill-turn-5")).toContainText("yourself");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Nothing is in the profile yet. (The facts' ids come from the interview itself.)
  const ids = await page.evaluate(async () => {
    const interview = (await (await fetch("/api/v1/profile/grill/0")).json()) as {
      proposals: { id: string; text: string }[];
    };
    const view = (await (await fetch("/api/v1/profile")).json()) as {
      profile: { work: { facts?: unknown[] }[] };
    };
    return {
      byText: Object.fromEntries(interview.proposals.map((fact) => [fact.text, fact.id])),
      kept: view.profile.work[0]?.facts?.length ?? 0,
    };
  });
  expect(ids.kept).toBe(0);
  const led = ids.byText["I proposed it and led it"] ?? "";
  const helped = ids.byText["2 others helped part time"] ?? "";
  const took = ids.byText["It took 7 months"] ?? "";
  expect([led, helped, took].every((id) => id !== "")).toBe(true);

  // Keep one as it is.
  await page.getByTestId(`grill-keep-${led}`).click();
  await expect(page.getByTestId(`grill-kept-${led}`)).toBeVisible();

  // Fix one: more than the person's words say is refused; what they say is kept.
  await page.getByTestId(`grill-fix-${helped}`).click();
  await page.getByTestId(`grill-fix-text-${helped}`).fill("9 others helped full time");
  await page.getByTestId(`grill-fix-save-${helped}`).click();
  await expect(page.getByTestId(`grill-fact-${helped}`)).toContainText("don’t say that");
  await expect(page.getByTestId(`grill-kept-${helped}`)).toHaveCount(0);
  await page.getByTestId(`grill-fix-text-${helped}`).fill("Two others helped part time");
  await page.getByTestId(`grill-fix-save-${helped}`).click();
  await expect(page.getByTestId(`grill-kept-${helped}`)).toBeVisible();
  await expect(page.getByTestId(`grill-fact-${helped}`)).toContainText(
    "Two others helped part time",
  );
  await expect(page.getByTestId(`grill-fact-${helped}`)).toContainText("2 others helped part time");

  // Drop the last.
  await page.getByTestId(`grill-drop-${took}`).click();
  await expect(page.getByTestId(`grill-dropped-${took}`)).toBeVisible();
  await expect(page.getByTestId("grill-keep-all-4")).toHaveCount(0);
  await expectNoSidewaysScroll(page);

  // The list remembers how far the role is.
  await page.goto("/profile/grill");
  await expect(page.getByTestId("grill-role-0")).toContainText("in progress");
  await expect(page.getByTestId("grill-role-0")).toContainText("1/7");

  // The kept facts are on the profile, under the role, with the person's words; Remove has Undo.
  await page.goto("/profile");
  const list = page.getByTestId("role-facts-list-0");
  await expect(list).toContainText("I proposed it and led it");
  await expect(list).toContainText("Two others helped part time");
  await expect(list).toContainText("2 others helped part time");
  await expect(list).not.toContainText("It took 7 months");
  await expect(page.getByTestId("role-facts-list-1")).toHaveCount(0);
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await page.getByTestId(`role-fact-remove-${led}`).click();
  await expect(list).not.toContainText("I proposed it and led it");
  await page.keyboard.press("z");
  await expect(page.getByTestId("role-facts-list-0")).toContainText("I proposed it and led it");

  // The role's own link goes back into the interview, where it left off.
  await page.getByTestId("role-grill-0").click();
  await expect(page).toHaveURL(/\/profile\/grill\/0$/);
  await expect(page.getByTestId("grill-turn-5")).toContainText("yourself");
  await expect(page.getByTestId("grill-answer")).toBeVisible();
  await expect(page.getByTestId("grill-coverage-count")).toHaveText("1/7");
  await expectNoSidewaysScroll(page);
});
