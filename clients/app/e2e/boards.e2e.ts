import { expect, test } from "@playwright/test";

import {
  boardBySlug,
  callApi,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  latestSearchStatus,
  signIn,
} from "./helpers";

/** An invented profile (nobody real), enough for a search to score against. */
const PROFILE = {
  basics: { label: "Senior software engineer", location: "Lisbon, Portugal", profiles: [] },
  work: [
    { name: "Example Org", position: "Senior Engineer", startDate: "2020-01", highlights: [] },
  ],
  education: [],
  skills: [{ name: "Rails" }, { name: "Platform" }, { name: "Rust" }],
  languages: [{ language: "English", fluency: "Full professional" }],
  certificates: [],
  projects: [],
};
const GOALS = { roles: ["Staff engineer"], work_mode: ["remote"] };

test("the boards searches read are ranked by salary, a candidate waits with its check, the Scanned view shows a search, and a match credits its board", async ({
  page,
}) => {
  await signIn(page);
  expect(await callApi(page, "PUT", "/api/v1/profile", PROFILE)).toBe(200);
  expect(await callApi(page, "PUT", "/api/v1/goals", GOALS)).toBe(200);
  // A search, so the Scanned view has something to show (the fake board).
  const started = await callApi(page, "POST", "/api/v1/searches");
  expect([202, 409]).toContain(started);
  await expect.poll(() => latestSearchStatus(page), { timeout: 60_000 }).toBe("finished");

  // The Boards page, from the sidebar or the tab bar.
  await page.goto("/boards");
  await expectPageFillsTheScreen(page, "boards");
  const approved = page.getByTestId("boards-section-approved");
  await expect(approved).toContainText("Fake Remote Board");
  await expect(approved).toContainText("avg");
  // Its jobs, asked for by the profile's top skills, say how well they fit them.
  await expect(approved).toContainText(/fits your skills \d+%/);
  const waiting = page.getByTestId("boards-section-waiting");
  await expect(waiting).toContainText("Fake Waiting Board");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // A candidate shows its check; the demo person owns the boards.
  await waiting.getByTestId("boards-row").first().click();
  await expect(page.getByTestId("board-peek")).toBeVisible();
  await expect(page.getByTestId("board-terms-sentences")).toContainText(
    "You may not scrape this site",
  );
  await expect(page.getByTestId("board-sample")).toContainText("Senior Backend Engineer");
  await expect(page.getByTestId("board-approve")).toBeVisible();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await page.getByTestId("board-peek-close").click();

  // Scanned: the search, with the board's line.
  await page.getByTestId("boards-tab-scanned").click();
  const run = page.getByTestId("scanned-run").first();
  await expect(run).toBeVisible();
  await expect(run.getByTestId("scanned-line").first()).toContainText("Fake Remote Board");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // A match credits the board it came from, with a link to the posting there.
  await page.goto("/pipeline");
  await page.getByTestId("pipeline-stage-filter").selectOption("all");
  const row = page.getByTestId("pipeline-row").first();
  await expect(row).toBeVisible();
  const phone = (page.viewportSize()?.width ?? 0) < 768;
  await (phone ? row.click() : row.getByTestId("pipeline-row-open").click());
  await expect(page.getByTestId("peek-source-link").first()).toContainText("Fake Remote Board");
  await expect(page.getByTestId("peek-source-link").first()).toHaveAttribute(
    "href",
    /^https:\/\/example\.com\/remote-board\/jobs\//,
  );
  await expect(page.getByTestId("peek-apply")).toHaveAttribute(
    "href",
    /^https:\/\/example\.com\/apply\/\d+\?utm_source=api$/,
  );
});

test("every waiting board can be approved or says why: checked at start, a key set, terms that forbid it, a proposed config, a config error beside Save", async ({
  page,
}, testInfo) => {
  await signIn(page);
  // Each Playwright project uses its own site with no config (the proposal is used once).
  const feedSite = testInfo.project.name === "phone" ? "phone" : "desktop";
  // Checked at start: the seeded board is approvable, the site's feed is proposed.
  await expect
    .poll(async () => (await boardBySlug(page, "fake-seeded"))?.approvable ?? false, {
      timeout: 60_000,
    })
    .toBe(true);
  await expect
    .poll(
      async () => {
        const site = await boardBySlug(page, `fake-feed-${feedSite}`);
        return site?.report?.proposal !== undefined && site.report.proposal !== null;
      },
      { timeout: 60_000 },
    )
    .toBe(true);
  // The keyed board starts with no key, whichever project ran first.
  const keyed = await boardBySlug(page, "fake-keyed");
  expect(keyed).not.toBeNull();
  expect(await callApi(page, "DELETE", `/api/v1/boards/${keyed?.board.id ?? ""}/key`)).toBe(200);

  await page.goto("/boards");
  const waiting = page.getByTestId("boards-section-waiting");
  const peek = page.getByTestId("board-peek");
  const open = async (name: string) => {
    await waiting.getByTestId("boards-row").filter({ hasText: name }).click();
    await expect(peek).toBeVisible();
    await expect(page.getByTestId("board-manage")).toBeVisible();
  };
  const close = async () => {
    await page.getByTestId("board-peek-close").click();
    await expect(peek).toBeHidden();
  };

  // Checked at start with its own config: nothing in the way.
  await open("Fake Seeded Board");
  await expect(page.getByTestId("board-approve")).toBeEnabled();
  await expect(page.getByTestId("board-blocked")).toHaveCount(0);
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // A config that doesn't read: the field named next to Save.
  await page.getByTestId("board-config-advanced").click();
  await page
    .getByTestId("board-config")
    .fill('{ "endpoint": "http://seeded-board.test:9020/api/jobs", "max_pages": "five" }');
  await page.getByTestId("board-config-save").click();
  await expect(page.getByTestId("board-config-error")).toContainText("`max_pages`");
  await expectNoSidewaysScroll(page);
  await close();

  // Its terms forbid automated reading: said, quoted, and Approve stays off.
  await open("Fake Forbidding Board");
  await expect(page.getByTestId("board-blocker-terms_forbid")).toContainText(
    "read this invented site with robots",
  );
  await expect(page.getByTestId("board-approve")).toBeDisabled();
  await close();

  // It needs a key: how to get one; once set, checked again and approvable.
  await open("Fake Keyed Board");
  await expect(page.getByTestId("board-blocker-needs_key")).toContainText(
    "Needs an API key from Fake Keyed Board",
  );
  await expect(page.getByTestId("board-approve")).toBeDisabled();
  await page.getByTestId("board-key-api_key").fill("e2e-key");
  await page.getByTestId("board-key-save").click();
  await expect(page.getByTestId("board-key-state")).toContainText("Key set");
  await expect(page.getByTestId("board-blocker-needs_key")).toHaveCount(0);
  await expect(page.getByTestId("board-key-api_key")).toHaveValue("");
  await page.getByTestId("board-check").click();
  await expect(page.getByTestId("board-check-started")).toBeVisible();
  await expect
    .poll(async () => (await boardBySlug(page, "fake-keyed"))?.approvable ?? false, {
      timeout: 30_000,
    })
    .toBe(true);
  await close();
  await open("Fake Keyed Board");
  await expect(page.getByTestId("board-approve")).toBeEnabled();
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await close();

  // A site with no config: the check found its RSS feed and proposes a config.
  await open(`Fake Feed Site (${feedSite})`);
  await expect(page.getByTestId("board-blocker-no_endpoint")).toBeVisible();
  const proposal = page.getByTestId("board-proposal-fields");
  await expect(proposal).toContainText("/jobs.rss");
  await expect(proposal).toContainText("Title");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await page.getByTestId("board-proposal-use").click();
  await expect(page.getByTestId("board-proposal")).toHaveCount(0);
  await expect(page.getByTestId("board-approve")).toBeEnabled();
});
