import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

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

/** The invented LinkedIn data export the sign-in spec imports too. */
const LINKEDIN_EXPORT = fileURLToPath(new URL("fixtures/linkedin-export.zip", import.meta.url));

/** Imports the export through the API, as the page uploads it, and merges it. */
async function importAndMerge(page: Page) {
  const zip = readFileSync(LINKEDIN_EXPORT).toString("base64");
  const id = await page.evaluate(async (zip) => {
    const bytes = Uint8Array.from(atob(zip), (c) => c.codePointAt(0) ?? 0);
    const form = new FormData();
    form.append("kind", "linkedin_zip");
    form.append("file", new Blob([bytes], { type: "application/zip" }), "export.zip");
    const response = await fetch("/api/v1/profile/imports", {
      method: "POST",
      headers: { "X-Applicamp-CSRF": "1" },
      body: form,
    });
    return ((await response.json()) as { id: string }).id;
  }, zip);
  expect(
    await callApi(page, "POST", `/api/v1/profile/imports/${id}/apply`, { mode: "merge" }),
  ).toBe(200);
}

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
  // A LinkedIn export read again and merged: the page says so for a day.
  await importAndMerge(page);

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
  await expect(page.getByTestId("complete-cv-read")).toContainText("We read your CV again.");
  await expect(page.getByTestId("complete-cv-read")).toContainText("Found 2 jobs");
  // Buttons big enough to tap: 44 px on a phone, 36 px on a desktop.
  const keep = await page.getByTestId(`complete-keep-${told.id}`).boundingBox();
  expect(keep?.height ?? 0).toBeGreaterThanOrEqual(phone ? 44 : 36);
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Keep one: it turns green and stays, with Undo; Undo puts it back.
  await page.getByTestId(`complete-keep-${told.id}`).click();
  await expect(page.getByTestId(`complete-fact-${told.id}`)).toContainText("Kept");
  await page.getByTestId(`complete-undo-${told.id}`).click();
  await expect(page.getByTestId(`complete-keep-${told.id}`)).toBeVisible();
  await page.getByTestId(`complete-keep-${told.id}`).click();
  await expect(page.getByTestId(`complete-undo-${told.id}`)).toBeVisible();
  // Drop one: it fades and stays, with Undo; Undo puts it back.
  await page.getByTestId(`complete-drop-${guessed.id}`).click();
  await expect(page.getByTestId(`complete-fact-${guessed.id}`)).toContainText("Dropped");
  await page.getByTestId(`complete-undo-${guessed.id}`).click();
  await expect(page.getByTestId(`complete-drop-${guessed.id}`)).toBeVisible();
  await page.getByTestId(`complete-drop-${guessed.id}`).click();
  await expect(page.getByTestId(`complete-undo-${guessed.id}`)).toBeVisible();
  await expect(page.getByTestId("complete-progress")).toContainText("decided");

  // Keep all: the rest at once, one line says how many were added, and one
  // Undo takes them all back for a day.
  await page.getByTestId("complete-keep-all").click();
  await expect(page.getByTestId("complete-done")).toContainText("facts added to your profile.");
  await expect(page.getByTestId("complete-done")).toContainText("You can undo this for a day.");
  await expect(page.getByTestId("complete-undo-all")).toBeVisible();
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
