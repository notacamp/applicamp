import { expect, test, type Page } from "@playwright/test";

import {
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

// Filmed: CI keeps this spec's videos (the `e2e-recordings` artifact), the
// recording of the three places at desktop width and at 375px.
test.use({ video: "on" });

/** A beat for whoever watches the recording; nothing waits on it. */
const beat = (page: Page) => page.waitForTimeout(800);

/** The link inside a test handle: where it is, what it says, whether it is the current page. */
interface Link {
  x: number;
  y: number;
  height: number;
  label: string;
  current: boolean;
}

/**
 * The link a test handle wraps (a nav item), or the tab bar's tab to `href`;
 * `null` while it is not on screen (the sidebar on a phone, the tab bar on a desktop).
 */
function linkIn(page: Page, testId: string, href?: string): Promise<Link | null> {
  return page.evaluate(
    ({ testId, href }) => {
      const link = document.querySelector(
        href === undefined
          ? `[data-testid="${testId}"] a`
          : `[data-testid="${testId}"] a[href="${href}"]`,
      );
      if (!(link instanceof HTMLElement) || link.getClientRects().length === 0) {
        return null;
      }
      const box = link.getBoundingClientRect();
      return {
        x: box.x + box.width / 2,
        y: box.y + box.height / 2,
        height: box.height,
        label: link.textContent?.trim() ?? "",
        current: link.getAttribute("aria-current") === "page",
      };
    },
    { testId, href },
  );
}

/** The h1 of the page with this test id. */
const heading = (page: Page, testId: string) =>
  page.evaluate(
    (testId) => document.querySelector(`[data-testid="${testId}"] h1`)?.textContent?.trim() ?? "",
    testId,
  );

test("Three places: Today, Jobs and You, no key hinted, the design system's own look, and the pages that left the nav keep their place", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";
  /** A place's link: its tab on a phone, its sidebar item on a desktop. */
  const place = (id: "today" | "pipeline" | "profile", href: "/" | "/pipeline" | "/profile") =>
    phone ? linkIn(page, "tab-bar", href) : linkIn(page, `nav-${id}`);
  /** Tap or click it. */
  const go = async (link: Link | null) => {
    expect(link).not.toBeNull();
    if (link !== null) {
      await page.mouse.click(link.x, link.y);
    }
  };

  await signIn(page);
  await expectPageFillsTheScreen(page, "today");
  await expect(page.getByTestId("today")).toContainText("Nothing is sent without your approval.");

  // The look: the package's fonts, and its accent, not a blue of the app's own.
  await expect
    .poll(() =>
      page.evaluate(async () => {
        await document.fonts.ready;
        return [...document.fonts].some(
          (face) => face.family.includes("Funnel Sans") && face.status === "loaded",
        );
      }),
    )
    .toBe(true);
  expect(await page.evaluate(() => getComputedStyle(document.body).fontFamily)).toContain(
    "Funnel Sans",
  );
  const accent = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue("--accent").trim(),
  );
  expect(accent).toContain("oklch");
  expect(accent).not.toContain("#165fae");

  // Three places, in this order, and nothing else; no key hinted anywhere.
  if (phone) {
    expect(
      await page.evaluate(() =>
        [...document.querySelectorAll('[data-testid="tab-bar"] a')].map(
          (tab) => tab.textContent?.trim() ?? "",
        ),
      ),
    ).toEqual(["Today", "Jobs", "You"]);
    expect(await linkIn(page, "nav-today")).toBeNull();
  } else {
    expect((await linkIn(page, "nav-today"))?.label).toBe("Today");
    expect((await linkIn(page, "nav-pipeline"))?.label).toBe("Jobs");
    expect((await linkIn(page, "nav-profile"))?.label).toBe("You");
    expect(await linkIn(page, "tab-bar", "/")).toBeNull();
  }
  for (const gone of ["nav-boards", "nav-companies", "nav-cvs", "nav-agents"]) {
    await expect(page.getByTestId(gone)).toHaveCount(0);
  }
  await expect(page.getByTestId("shortcut-sheet-button")).toHaveCount(0);
  expect(await page.evaluate(() => document.querySelectorAll("nav kbd, header kbd").length)).toBe(
    0,
  );
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await beat(page);

  // Jobs: big enough to tap (44px on a phone, 36px on a desktop), and the place marked.
  const jobs = await place("pipeline", "/pipeline");
  expect(jobs?.height ?? 0).toBeGreaterThanOrEqual(phone ? 44 : 36);
  await go(jobs);
  await expect(page).toHaveURL(/\/pipeline$/);
  await expectPageFillsTheScreen(page, "pipeline");
  expect(await heading(page, "pipeline")).toBe("Jobs");
  await expect.poll(async () => (await place("pipeline", "/pipeline"))?.current).toBe(true);
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await beat(page);

  // You.
  await go(await place("profile", "/profile"));
  await expect(page).toHaveURL(/\/profile$/);
  await expectPageFillsTheScreen(page, "profile");
  expect(await heading(page, "profile")).toBe("You");
  await expect.poll(async () => (await place("profile", "/profile"))?.current).toBe(true);
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);
  await beat(page);

  // The pages that left the nav keep their addresses and the place they are
  // reached from: Agents counts as Today, Job sites as You.
  await page.goto("/agents");
  await expectPageFillsTheScreen(page, "agents");
  await expect.poll(async () => (await place("today", "/"))?.current).toBe(true);
  expect((await place("profile", "/profile"))?.current).toBe(false);
  await beat(page);
  await page.goto("/boards");
  await expectPageFillsTheScreen(page, "boards");
  await expect.poll(async () => (await place("profile", "/profile"))?.current).toBe(true);
  expect((await place("today", "/"))?.current).toBe(false);
  await beat(page);

  // Back to Today from the nav.
  await go(await place("today", "/"));
  await expect(page).toHaveURL(/:\d+\/$/);
  await expect(page.getByTestId("today")).toBeVisible();
  await beat(page);
});
