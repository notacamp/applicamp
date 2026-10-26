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

test("an invited person signs in with Campbooks, lands on Today, finds Pipeline with ⌘K and signs out", async ({
  page,
}) => {
  // Signed out, the app sends the browser to the sign-in page.
  await page.goto("/");
  await expect(page).toHaveURL(/\/sign-in$/);
  const campbooks = page.getByTestId("sign-in-campbooks");
  await expect(campbooks).toBeVisible();
  await expectAccessible(page);

  // The fake Campbooks approves at once and the server signs the invited address in.
  await campbooks.click();
  await expect(page).toHaveURL(/:\d+\/$/);
  await expect(page.getByTestId("today")).toBeVisible();
  await expect(page.getByTestId("today-empty")).toBeVisible();
  await expectAccessible(page);

  // ⌘K (Ctrl+K here): type a place, Enter goes there.
  await page.keyboard.press("ControlOrMeta+k");
  await expect.poll(() => focusedRole(page)).toBe("combobox");
  await page.keyboard.type("Pipeline");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/pipeline$/);
  await expect(page.getByTestId("pipeline-empty")).toBeVisible();

  // Sign out from the account menu: the sign-in page, and signed out for real.
  await page.getByTestId("account-menu").click();
  await page.getByTestId("sign-out").click();
  await expect(page).toHaveURL(/\/sign-in$/);
  await expect(page.getByTestId("sign-in-campbooks")).toBeVisible();
  await page.goto("/pipeline");
  await expect(page).toHaveURL(/\/sign-in$/);
});
