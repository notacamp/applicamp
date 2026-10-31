import { expect, test } from "@playwright/test";

import {
  callApi,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

/** An invented profile (nobody real): two roles, one with a fact the person kept. */
function profile(start: string) {
  return {
    basics: { name: "Alex Demo", label: "Engineer", profiles: [] },
    work: [
      {
        name: "Example Co",
        position: "Senior Engineer",
        startDate: start,
        summary: "Runs the billing platform.",
        highlights: [],
        facts: [
          {
            id: `e2e-about-${start}`,
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
        highlights: [],
      },
    ],
    education: [],
    skills: [{ name: "PostgreSQL" }],
    languages: [],
    certificates: [],
    projects: [],
  };
}

const EXTRA =
  "I ran events and a restaurant for 6 years. I want a senior Rails role, ideally for restaurants. " +
  "I use Rails, Elixir and Postgres. I led 4 engineers at Example Co.";

test("someone reads what Applicamp knows and assumes about them, settles two assumptions, adds what else to know, resolves a flag with their own words, reviews and saves it all", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  // The desktop and phone runs (and a retry) share one stack and one person: a
  // start date of their own makes each run start from a new profile version.
  const month = (phone ? 6 : 3) + testInfo.retry;
  expect(
    await callApi(
      page,
      "PUT",
      "/api/v1/profile",
      profile(`2021-${String(month).padStart(2, "0")}`),
    ),
  ).toBe(200);
  // The employer's own (fake) website is known, so it is read: nothing else is fetched.
  expect(
    await callApi(page, "PUT", "/api/v1/profile/about/employers/0/site", {
      url: "http://example-co.test:9020/",
    }),
  ).toBe(200);

  // Profile & goals: "Tell me about you" is the way in.
  await page.goto("/profile");
  await expect(page.getByTestId("strength-about")).toBeVisible();
  await page.getByTestId("strength-about").click();
  await expect(page).toHaveURL(/\/profile\/about$/);
  await expect(page.getByTestId("about-portrait")).toBeVisible({ timeout: 30_000 });
  await expectPageFillsTheScreen(page, "about");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // The portrait shows all three kinds, and says what was read on the employer's site.
  const sentence = (text: string) => page.getByTestId(/^about-sentence-/).filter({ hasText: text });
  await expect(sentence("You were Senior Engineer at Example Co.")).toHaveAttribute(
    "data-kind",
    "known",
  );
  await expect(sentence("You told me: Moved invoicing onto PostgreSQL")).toHaveAttribute(
    "data-kind",
    "told",
  );
  await expect(sentence("according to its own website").first()).toHaveAttribute(
    "data-kind",
    "assumed",
  );
  // A claim no source makes is not dropped: it is an assumption now.
  const kube = sentence("Kubernetes");
  await expect(kube).toHaveAttribute("data-kind", "assumed");
  await kube.click();
  await expect(page.getByTestId("about-panel")).toContainText(
    "This isn’t in your profile in these words.",
  );

  // An assumption is a button: the keyboard opens it, and it says why.
  const led = sentence("I assume you led a small team at Example Co");
  await led.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("about-panel")).toContainText("Why:");
  await expect(page.getByTestId("about-panel")).toContainText("doesn’t say how many");
  await expect(page.getByTestId("about-claim")).toHaveText(
    "If right, I’ll save: Led a small team at Example Co (3 to 6 people)",
  );
  await expectNoSidewaysScroll(page);
  await page.getByTestId("about-choice-right").click();
  await expect(led).toHaveAttribute("data-kind", "confirmed");

  // One assumption is wrong: struck out.
  const wrong = sentence("usually run on bookings, payments and building access");
  await wrong.click();
  await page.getByTestId("about-choice-wrong").click();
  await expect(wrong).toHaveAttribute("data-kind", "wrong");

  // "Is this accurate?" is stored; "Anything else" is read.
  await page.getByTestId("about-accuracy-some").click();
  await expect(page.getByTestId("about-accuracy-some")).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("about-extra").fill(EXTRA);
  await page.getByTestId("about-continue").click();

  // What was added, with the person's own words; the claim their words don't say is flagged.
  await expect(page.getByTestId("about-added")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId("about-added")).toContainText("Elixir");
  await expect(page.getByTestId("about-added")).toContainText("Your words:");
  await expect(page.getByTestId("about-flags-c1")).toContainText("“Acme” isn’t in your words.");
  await expect(page.getByTestId("about-added-step")).toContainText(
    "Questions about the gaps come next.",
  );
  // The new sentences are in the portrait, highlighted as new.
  await expect(page.getByTestId("about-sentence-tc2")).toHaveAttribute("data-fresh", "");
  await expect(page.getByTestId("about-sentence-tc2")).toHaveAttribute("data-kind", "told");
  await expectNoSidewaysScroll(page);

  // Nothing is saved yet.
  const before = await page.evaluate(async () => {
    const view = (await (await fetch("/api/v1/profile")).json()) as {
      profile: { basics: { summary?: string }; work: { facts?: unknown[] }[] };
    };
    return {
      summary: view.profile.basics.summary ?? "",
      facts: view.profile.work[0]?.facts?.length ?? 0,
    };
  });
  expect(before).toEqual({ summary: "", facts: 1 });

  // "Use my own words": the summary becomes exactly what they wrote.
  await page.getByTestId("about-own-words-c1").click();
  await expect(page.getByTestId("about-flags-c1")).toHaveCount(0);

  // Review: every line, grouped, each marked; then Save.
  await page.getByTestId("about-to-review").click();
  await expect(page.getByTestId("about-review-step")).toBeVisible();
  for (const group of ["summary", "headline", "skills", "position-0", "background", "goals"]) {
    await expect(page.getByTestId(`about-group-${group}`)).toBeVisible();
  }
  await expect(page.getByTestId("about-group-position-0")).toContainText("Confirmed");
  await expect(page.getByTestId("about-group-position-0")).toContainText("Led 4 engineers");
  await expect(page.getByTestId("about-group-position-0")).toContainText(
    "Led a small team at Example Co (3 to 6 people)",
  );
  await expect(page.getByTestId("about-group-summary")).toContainText("You told me");
  await expect(page.getByTestId("about-left-out")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await page.getByTestId("about-save").click();
  await expect(page.getByTestId("about-saved")).toContainText("% strong");
  await expectNoSidewaysScroll(page);

  // Back on the profile: the summary, skills and background, and the confirmed assumption as a fact.
  await page.getByTestId("about-saved-done").click();
  await expect(page).toHaveURL(/\/profile$/);
  const saved = await page.evaluate(async () => {
    const view = (await (await fetch("/api/v1/profile")).json()) as {
      profile: {
        basics: { summary?: string; label?: string; background?: string };
        skills: { name: string }[];
        work: { facts?: { text: string; status?: string }[] }[];
      };
    };
    return view.profile;
  });
  expect(saved.basics.summary).toBe(EXTRA);
  expect(saved.basics.label).toBe("Senior Rails engineer");
  expect(saved.basics.background).toBe("I ran events and a restaurant for 6 years");
  // Postgres is PostgreSQL: not a second skill.
  expect(saved.skills.map((skill) => skill.name)).toEqual(["PostgreSQL", "Rails", "Elixir"]);
  const facts = saved.work[0]?.facts ?? [];
  expect(facts.map((fact) => fact.text)).toContain("Led 4 engineers");
  const confirmed = facts.find((fact) => fact.status === "confirmed_assumption");
  expect(confirmed?.text).toBe("Led a small team at Example Co (3 to 6 people)");
  expect(facts.some((fact) => fact.text.includes("bookings, payments"))).toBe(false);
  await expect(page.getByTestId("basics-summary")).toContainText("I ran events");
  await expect(page.getByTestId("basics-background")).toContainText("6 years");
  await expect(page.getByTestId("role-facts-list-0")).toContainText(
    "I assume you led a small team",
  );
  await expectNoSidewaysScroll(page);
});
