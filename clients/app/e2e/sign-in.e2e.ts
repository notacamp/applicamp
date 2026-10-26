import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

/** No WCAG 2.1 A/AA violation on the page as it is now. */
async function expectAccessible(page: Page) {
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
const focusedRole = (page: Page) =>
  page.evaluate(() => document.activeElement?.getAttribute("role") ?? null);

/** The page (`main`, by its test id) fills the screen's width, inside the viewport. */
async function expectPageFillsTheScreen(page: Page, testId: string) {
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

test("an invited person signs in with Campbooks, lands on Today, finds Pipeline with ⌘K and signs out", async ({
  page,
}, testInfo) => {
  const phone = testInfo.project.name === "phone";

  // Signed out, the app sends the browser to the sign-in page.
  await page.goto("/");
  await expect(page).toHaveURL(/\/sign-in$/);
  const campbooks = page.getByTestId("sign-in-campbooks");
  await expect(campbooks).toBeVisible();
  await expect(campbooks).toBeInViewport();
  await expectAccessible(page);

  // The fake Campbooks approves at once and the server signs the invited address in.
  await campbooks.click();
  await expect(page).toHaveURL(/:\d+\/$/);
  await expectPageFillsTheScreen(page, "today");
  await expect(page.getByTestId("today-empty")).toBeVisible();
  await expect(page.getByTestId("today-empty")).toBeInViewport();
  await expectAccessible(page);

  // The command menu: ⌘K (Ctrl+K here) on a desktop, the search button on a
  // phone. Type a place, Enter goes there.
  await (phone ? page.getByTestId("jump-phone").click() : page.keyboard.press("ControlOrMeta+k"));
  await expect.poll(() => focusedRole(page)).toBe("combobox");
  await page.keyboard.type("Pipeline");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/pipeline$/);
  await expectPageFillsTheScreen(page, "pipeline");
  await expect(page.getByTestId("pipeline-empty")).toBeVisible();

  // Sign out from the account menu (the avatar in the phone header on a
  // phone): the sign-in page, and signed out for real.
  await page.getByTestId(phone ? "account-menu-phone" : "account-menu").click();
  await page.getByTestId("sign-out").click();
  await expect(page).toHaveURL(/\/sign-in$/);
  await expect(page.getByTestId("sign-in-campbooks")).toBeVisible();
  await page.goto("/pipeline");
  await expect(page).toHaveURL(/\/sign-in$/);
});
