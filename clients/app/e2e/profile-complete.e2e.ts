import { expect, test, type Page } from "@playwright/test";

import {
  callApi,
  callApiJson,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

// Filmed: CI keeps this spec's videos (the `e2e-recordings` artifact), the
// recording of Complete your profile at desktop width and at 375px.
test.use({ video: "on" });

/** An invented profile (nobody real): a lead role and an earlier one. */
function profile(start: string) {
  return {
    basics: { name: "Alex Demo", label: "Engineer", profiles: [] },
    work: [
      {
        name: "Example Co",
        position: "Lead Engineer",
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
    skills: [{ name: "PostgreSQL" }],
    languages: [],
    certificates: [],
    projects: [],
  };
}

/** What the person tells Applicamp under "Anything else" (invented). */
const EXTRA =
  "I ran events and a restaurant for 6 years. I want a senior Rails role. " +
  "I use Rails, Elixir and Postgres. I led 4 engineers at Example Co.";

interface Proposal {
  id: string;
  text: string;
  state: string;
}

interface Proposals {
  groups: { start_date?: string | null; proposals: Proposal[] }[];
  open: number;
}

/** The open proposal saying `text` on the position that started `start`. */
async function proposalOn(page: Page, start: string, text: string): Promise<Proposal | undefined> {
  const { body } = await callApiJson(page, "GET", "/api/v1/profile/facts/proposals");
  return (body as Proposals).groups
    .filter((group) => group.start_date === start)
    .flatMap((group) => group.proposals)
    .find((proposal) => proposal.text === text && proposal.state === "open");
}

test("someone checks the facts the app found from Tell me about you: keeps one, drops one, keeps the rest, and can undo for a day", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  // The desktop and phone runs (and a retry) share one stack and one person: a
  // start date of their own makes each run a new position, with its own facts.
  const month = (phone ? 8 : 5) + testInfo.retry;
  const start = `2021-${String(month).padStart(2, "0")}`;
  expect(await callApi(page, "PUT", "/api/v1/profile", profile(start))).toBe(200);

  // Tell me about you, through the API with the fake model: the portrait, "Is
  // this accurate? Yes, mostly", and what else to know. Nothing is saved.
  expect((await callApiJson(page, "POST", "/api/v1/profile/about/portrait?locale=en")).status).toBe(
    200,
  );
  expect(await callApi(page, "PUT", "/api/v1/profile/about/accuracy", { accuracy: "yes" })).toBe(
    200,
  );
  expect(
    (await callApiJson(page, "POST", "/api/v1/profile/about/extra?locale=en", { text: EXTRA }))
      .status,
  ).toBe(200);

  // The app proposes the facts by itself, in the background, with no model.
  const TOLD = "Led 4 engineers";
  const GUESSED = "You also run Kubernetes clusters in production.";
  await expect
    .poll(
      async () =>
        (await proposalOn(page, start, TOLD)) !== undefined &&
        (await proposalOn(page, start, GUESSED)) !== undefined,
      { timeout: 30_000 },
    )
    .toBe(true);
  const told = await proposalOn(page, start, TOLD);
  const guessed = await proposalOn(page, start, GUESSED);
  if (told === undefined || guessed === undefined) {
    throw new Error("no facts were proposed");
  }

  // Profile & goals says how many facts wait, and leads to them.
  await page.goto("/profile");
  await expect(page.getByTestId("profile-facts-nudge")).toContainText("new facts for your profile");
  await page.getByTestId("profile-facts-open").click();
  await expect(page).toHaveURL(/\/profile\/complete$/);
  await expectPageFillsTheScreen(page, "profile-complete");
  await expect(page.getByTestId("complete-group-0")).toContainText("Example Co · 2021 – now");
  await expect(page.getByTestId(`complete-fact-${told.id}`)).toContainText("you said");
  await expect(page.getByTestId(`complete-fact-${told.id}`)).toContainText("“led 4 engineers”");
  await expect(page.getByTestId(`complete-fact-${guessed.id}`)).toContainText("you confirmed");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Keep one: it turns green and stays, with Undo.
  await page.getByTestId(`complete-keep-${told.id}`).click();
  await expect(page.getByTestId(`complete-fact-${told.id}`)).toContainText("Kept");
  await expect(page.getByTestId(`complete-undo-${told.id}`)).toBeVisible();
  // Drop one: it fades and stays, with Undo.
  await page.getByTestId(`complete-drop-${guessed.id}`).click();
  await expect(page.getByTestId(`complete-fact-${guessed.id}`)).toContainText("Dropped");
  await expect(page.getByTestId(`complete-undo-${guessed.id}`)).toBeVisible();
  await expect(page.getByTestId("complete-progress")).toContainText("decided");

  // Keep all: the rest at once, and one line says how many were added.
  await page.getByTestId("complete-keep-all").click();
  await expect(page.getByTestId("complete-done")).toContainText("facts added to your profile.");
  await expect(page.getByTestId("complete-done")).toContainText("You can undo this for a day.");
  await expect(page.getByTestId(`complete-fact-${guessed.id}`)).toContainText("Dropped");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // The kept fact is on the job, in the person's own words.
  const { body } = await callApiJson(page, "GET", "/api/v1/profile");
  const facts = (body as { profile: { work: { facts?: { text: string; quote: string }[] }[] } })
    .profile.work[0]?.facts;
  expect(facts?.find((fact) => fact.text === TOLD)?.quote).toBe("led 4 engineers");
  expect(facts?.some((fact) => fact.text.includes("Kubernetes"))).toBe(false);
});
