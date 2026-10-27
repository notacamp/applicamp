import AxeBuilder from "@axe-core/playwright";
import { expect, type Page } from "@playwright/test";

/**
 * Every finite animation and transition on the page has ended (a side peek
 * sliding in): colours are checked as they end up, not half-faded.
 */
export async function animationsDone(page: Page) {
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((animation) => animation.effect?.getTiming().iterations !== Infinity)
        .map((animation) => animation.finished.catch(() => null)),
    ),
  );
}

/** No WCAG 2.1 A/AA violation on the page as it is now (once its animations end). */
export async function expectAccessible(page: Page) {
  await animationsDone(page);
  const { violations } = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
    .analyze();
  const found = violations.map(
    ({ id, impact, nodes }) =>
      `${id} (${impact ?? "?"}): ${nodes.map((node) => node.target.join(" ")).join(", ")}`,
  );
  expect(found).toEqual([]);
}

/** The ARIA role of the element that has the focus. */
export const focusedRole = (page: Page) =>
  page.evaluate(() => document.activeElement?.getAttribute("role") ?? null);

/** The page (`main`, by its test id) fills the screen's width, inside the viewport. */
export async function expectPageFillsTheScreen(page: Page, testId: string) {
  const main = page.getByTestId(testId);
  await expect(main).toBeInViewport();
  const box = await main.boundingBox();
  const width = page.viewportSize()?.width ?? 0;
  expect(box).not.toBeNull();
  if (box !== null) {
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(width + 1);
    expect(box.width).toBeGreaterThan(Math.min(300, width - 1));
  }
}

/** The document never scrolls sideways (a phone's 375px included). */
export async function expectNoSidewaysScroll(page: Page) {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
}

/** Signed out, sign in with the fake Campbooks and land on Today. */
export async function signIn(page: Page) {
  await page.goto("/");
  await expect(page).toHaveURL(/\/sign-in$/);
  await page.getByTestId("sign-in-campbooks").click();
  await expect(page).toHaveURL(/:\d+\/$/);
  await expect(page.getByTestId("today")).toBeVisible();
}

/**
 * Calls the API from the signed-in page (the session cookie and the CSRF
 * header, as the app sends them); the answer's status.
 */
export function callApi(page: Page, method: string, path: string, body?: unknown) {
  return page.evaluate(
    async ({ method, path, body }) =>
      (
        await fetch(path, {
          method,
          headers: { "content-type": "application/json", "X-Applicamp-CSRF": "1" },
          ...(body === undefined ? {} : { body: JSON.stringify(body) }),
        })
      ).status,
    { method, path, body },
  );
}

/** The latest search's status, as the API says it (`null` before any). */
export function latestSearchStatus(page: Page) {
  return page.evaluate(async () => {
    const answer = (await (await fetch("/api/v1/searches/latest")).json()) as {
      search?: { status?: string } | null;
    };
    return answer.search?.status ?? null;
  });
}
