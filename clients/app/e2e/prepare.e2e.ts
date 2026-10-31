import { expect, test } from "@playwright/test";

import {
  callApi,
  callApiJson,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

/** An invented profile (nobody real): two roles, the second the older one. */
function profile() {
  return {
    basics: { profiles: [] },
    work: [
      {
        name: "Example Org",
        position: "Senior Engineer",
        startDate: "2021-03",
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

test("someone prepares a job: reads the company's own pages, removes a wrong fact and undoes it, sees what the job wants, answers one question into a fact they keep and puts a skill away", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  await signIn(page);
  expect(await callApi(page, "PUT", "/api/v1/profile", profile())).toBe(200);

  // The desktop and phone runs (and a retry) share one stack and one person: a
  // company of their own makes each start from nothing (its facts are cached).
  const tag = `${testInfo.project.name}-${String(testInfo.retry)}-${Date.now().toString(36)}`;
  const company = `Example Co ${tag}`;
  // A posting on the company's own site (a fake, with a robots.txt that forbids /private).
  const added = await callApiJson(page, "POST", "/api/v1/opportunities", {
    // The skill it asks for is the run's own: one put away is not asked about again.
    url: `http://example-co.test:9020/jobs/staff-engineer?run=${tag}&skill=Hotwire-${tag}`,
    title: "Staff Engineer",
    company,
  });
  expect(added.status).toBe(201);
  const id = (added.body as { id: string }).id;
  expect(
    (await callApiJson(page, "PATCH", `/api/v1/opportunities/${id}`, { stage: "shortlisted" }))
      .status,
  ).toBe(200);

  // The peek's Tailored CV block: the way in, with what it costs.
  await page.goto("/pipeline");
  await page.getByTestId("pipeline-search").fill(company);
  const row = page.getByTestId("pipeline-row").first();
  await expect(row).toBeVisible();
  await (phone ? row.click() : row.getByTestId("pipeline-row-open").click());
  await expect(page.getByTestId("peek-tailored")).toContainText("real facts");
  await expect(page.getByTestId("peek-tailored")).toContainText("€0.004");
  await page.getByTestId("peek-prepare").click();
  await expect(page).toHaveURL(new RegExp(`/pipeline/${id}/prepare$`));
  await expectPageFillsTheScreen(page, "prepare");

  // Nothing has been read: the site is the posting's own host, Read says what it costs.
  await expect(page.getByTestId("prepare-site")).toContainText("example-co.test");
  await expect(page.getByTestId("prepare-read")).toContainText(`Read about ${company}`);
  await expect(page.getByTestId("prepare-read")).toContainText("€0.004");
  await expect(page.getByTestId("prepare-draft-slot")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Read: facts that each quote a page of the company's site, with their sources.
  await page.getByTestId("prepare-read").click();
  const facts = page.getByTestId("prepare-facts");
  await expect(facts).toBeVisible();
  await expect(facts).toContainText("Builds ledgers for small shops");
  await expect(facts).toContainText("Is moving its dashboard from React to Hotwire");
  await expect(facts).toContainText("moving our dashboard from React to Hotwire");
  // The fact whose quote is on no page was left out, and the forbidden page was not read.
  await expect(facts).not.toContainText("900");
  await expect(page.getByTestId("prepare-dropped")).toContainText("1 fact was left out");
  await expect(page.getByTestId("prepare-blocked")).toContainText("1 page was left alone");
  await expect(page.getByTestId("prepare")).not.toContainText("SECRET ROADMAP");
  const pages = page.getByTestId("prepare-pages");
  await expect(pages).toContainText("example-co.test:9020/about");
  await expect(pages).toContainText("example-co.test:9020/blog/hotwire");
  await expect(pages).not.toContainText("/private");
  await expect(page.getByTestId(/^prepare-fact-source-/).first()).toHaveAttribute(
    "href",
    /^http:\/\/example-co\.test:9020\//,
  );
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Remove one fact, then Undo.
  const hotwire = facts.getByTestId(/^prepare-fact-[0-9a-f-]{36}$/).filter({
    hasText: "Is moving its dashboard",
  });
  await hotwire.getByTestId(/^prepare-fact-remove-/).click();
  await expect(hotwire).toContainText("Removed");
  await hotwire.getByTestId(/^prepare-fact-undo-/).click();
  await expect(hotwire.getByTestId(/^prepare-fact-remove-/)).toBeVisible();

  // What the job wants most, and the questions where the profile is thin.
  await expect(page.getByTestId("prepare-want-0")).toContainText("Rails");
  await expect(page.getByTestId("prepare-want-2")).toContainText("Kafka · nice to have");
  const questions = page.getByTestId(/^prepare-question-[0-9a-f-]{36}$/);
  await expect(questions).toHaveCount(3);
  await expect(questions.first()).toContainText("Hotwire");
  await expect(questions.first()).toContainText(
    "Your answer goes on Senior Engineer · Example Org.",
  );
  await expectNoSidewaysScroll(page);

  // Answer the one that names no position: it goes on the most recent one, and says so.
  const owning = questions.filter({ hasText: "own billing" });
  await expect(owning.getByTestId(/^prepare-question-goes-/)).toContainText("most recent");
  await expect(owning.getByTestId(/^prepare-donthave-/)).toHaveCount(0);
  await owning
    .getByTestId(/^prepare-answer-[0-9a-f-]{36}$/)
    .fill("I owned invoicing. 3 of us shipped it in 5 weeks.");
  await owning.getByTestId(/^prepare-answer-send-/).click();
  const answered = page.getByTestId(/^prepare-answered-/);
  await expect(answered).toContainText("your most recent position: Senior Engineer · Example Org");
  await expect(answered).toContainText("1 line was left out");
  const proposals = answered.getByTestId(/^grill-fact-[0-9a-f-]{36}$/);
  await expect(proposals).toHaveCount(2);
  await expect(proposals.first()).toContainText("Your words:");
  const invoicing = proposals.filter({ hasText: "I owned invoicing" });
  await invoicing.getByTestId(/^grill-keep-/).click();
  await expect(invoicing.getByTestId(/^grill-kept-/)).toBeVisible();
  await expectNoSidewaysScroll(page);

  // "I don't have it" on the skill question: it is gone, and so is its skill elsewhere.
  const skill = questions.filter({ hasText: "worked with Hotwire" });
  await skill.getByTestId(/^prepare-donthave-/).click();
  await expect(questions.filter({ hasText: "worked with Hotwire" })).toHaveCount(0);

  // Reading again within 30 days: Read again fetches once more, and a removed fact stays removed.
  await hotwire.getByTestId(/^prepare-fact-remove-/).click();
  await expect(hotwire).toContainText("Removed");
  await page.getByTestId("prepare-read-again").click();
  await expect(hotwire).toContainText("Removed");
  await expect(page.getByTestId("prepare-error")).toHaveCount(0);

  // The kept fact is on the profile, under the role the answer went to.
  await page.goto("/profile");
  const list = page.getByTestId("role-facts-list-0");
  await expect(list).toContainText("I owned invoicing");
  await expect(list).not.toContainText("3 of us shipped");
  await expect(page.getByTestId("role-facts-list-1")).toHaveCount(0);
  await expectNoSidewaysScroll(page);
});

test("a posting on a job board's own tracker says the company's website is needed, once", async ({
  page,
}, testInfo) => {
  await signIn(page);
  const tag = `${testInfo.project.name}-${String(testInfo.retry)}-${Date.now().toString(36)}`;
  const added = await callApiJson(page, "POST", "/api/v1/opportunities", {
    url: `https://careers.example.org/${tag}/jobs/42`,
    title: "Platform Engineer",
    company: `Tracker Co ${tag}`,
  });
  const id = (added.body as { id: string }).id;
  await callApiJson(page, "PATCH", `/api/v1/opportunities/${id}`, { stage: "shortlisted" });
  await page.goto(`/pipeline/${id}/prepare`);
  // Its own link counts: the careers site's host is the company's.
  await expect(page.getByTestId("prepare-site")).toContainText("example.org");
  // A website that cannot be read is refused in plain words.
  await page.getByTestId("prepare-site-change").click();
  await page.getByTestId("prepare-site-input").fill("not a website");
  await page.getByTestId("prepare-site-save").click();
  await expect(page.getByTestId("prepare-site-input")).toHaveAttribute("aria-invalid", "true");
});
