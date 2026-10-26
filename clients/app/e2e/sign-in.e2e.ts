import { fileURLToPath } from "node:url";

import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

/**
 * An invented LinkedIn data export (Alex Demo at Example Org and Sample
 * Labs; nobody real): Profile, Positions, Education, Skills, Languages,
 * Certifications, Projects and Email Addresses CSVs.
 */
const LINKEDIN_EXPORT = fileURLToPath(new URL("fixtures/linkedin-export.zip", import.meta.url));

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

test("an invited person signs in with Campbooks, lands on Today, imports their LinkedIn export, answers a few goals questions, finds Pipeline with ⌘K and signs out", async ({
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

  // "Set up your profile" opens the import panel. With no AI model on this
  // stack, it offers the LinkedIn export only; the server reads its CSVs.
  await page.getByTestId("set-up-profile").click();
  await expect(page).toHaveURL(/\/profile\?import=new$/);
  await expectPageFillsTheScreen(page, "profile");
  await expect(page.getByTestId("import-zip-only")).toBeVisible();
  await page.getByTestId("import-file").setInputFiles(LINKEDIN_EXPORT);
  await page.getByTestId("import-submit").click();

  // The draft, to review: nothing is saved yet. Replace makes it the profile.
  await expect(page.getByTestId("import-review")).toBeVisible();
  await expect(page.getByTestId("review-work")).toContainText("Senior Engineer");
  await expectAccessible(page);
  await page.getByTestId("review-replace").click();
  const work = page.getByTestId("section-work");
  await expect(work).toContainText("Senior Engineer");
  await expect(work).toContainText("Example Org");
  await expect(work).toContainText("Sample Labs");
  await expect(page.getByTestId("profile-source")).toContainText("LinkedIn data export");
  await expectPageFillsTheScreen(page, "profile");
  await expectAccessible(page);

  // The profile is saved, and Profile & goals offers a few questions (never
  // forced). One per screen: roles are answered (Shift+Enter adds one, Enter
  // saves and goes on), the next four are skipped, and so is the salary.
  await expect(page.getByTestId("goals-offer")).toBeVisible();
  await page.getByTestId("goals-offer-start").click();
  await expect(page).toHaveURL(/\/goals\/interview\?step=roles$/);
  await expect(page.getByTestId("interview-roles")).toContainText("Question 1 of 11");
  await expect(page.getByTestId("interview-next")).toBeInViewport();
  await expectAccessible(page);
  await page.getByTestId("goal-roles-input").fill("Staff engineer");
  await page.keyboard.press("Shift+Enter");
  await expect(page.getByTestId("goal-roles-item-0")).toContainText("Staff engineer");
  await page.getByTestId("goal-roles-input").fill("Founding engineer");
  await page.keyboard.press("Enter");
  for (const question of ["seniority", "work_mode", "locations", "relocate"]) {
    await expect(page.getByTestId(`interview-${question}`)).toBeVisible();
    await page.getByTestId("interview-skip").click();
  }
  await expect(page.getByTestId("interview-cash_floor")).toBeVisible();
  await expect(page.getByTestId("interview-next")).toBeInViewport();
  await page.getByTestId("interview-skip").click();
  // With no cash floor, equity is not asked: the company question is next.
  await expect(page.getByTestId("interview-company_stage")).toBeVisible();

  // Finish later leaves for Today, with the roles saved and the salary unanswered.
  await page.getByTestId("interview-later").click();
  await expect(page).toHaveURL(/:\d+\/$/);
  await expect(page.getByTestId("today")).toBeVisible();
  await page.goto("/profile");
  await expectPageFillsTheScreen(page, "profile");
  await expect(page.getByTestId("goal-roles")).toContainText("Staff engineer, Founding engineer");
  await expect(page.getByTestId("goal-cash_floor")).toContainText("Add your cash floor");
  await expect(page.getByTestId("goals-continue")).toBeVisible();
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
