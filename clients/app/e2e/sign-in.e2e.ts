import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

import { expectAccessible, expectPageFillsTheScreen, focusedRole } from "./helpers";

/**
 * An invented LinkedIn data export (Alex Demo at Example Org and Sample
 * Labs; nobody real): Profile, Positions, Education, Skills, Languages,
 * Certifications, Projects and Email Addresses CSVs.
 */
const LINKEDIN_EXPORT = fileURLToPath(new URL("fixtures/linkedin-export.zip", import.meta.url));

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
  await expectAccessible(page);

  // Both projects share one stack and one person: start with no goals answered, as a new person would.
  const cleared = await page.evaluate(
    async () =>
      (
        await fetch("/api/v1/goals", {
          method: "PUT",
          headers: { "content-type": "application/json", "X-Applicamp-CSRF": "1" },
          body: "{}",
        })
      ).status,
  );
  expect(cleared).toBe(200);

  // Today follows where the person is. A first visit offers "Set up your
  // profile", which opens the import panel; once another test has set a
  // profile, Today asks for the next step instead, so go to the panel.
  await expect(page.getByTestId("today-loading")).toHaveCount(0);
  const setUp = page.getByTestId("set-up-profile");
  if (await setUp.isVisible()) {
    await expect(page.getByTestId("today-setup")).toBeInViewport();
    await setUp.click();
  } else {
    await page.goto("/profile?import=new");
  }
  await expect(page).toHaveURL(/\/profile\?import=new$/);
  await expectPageFillsTheScreen(page, "profile");
  // This stack has an AI model (the fake), so PDFs and text are offered too;
  // the LinkedIn export is the default, read from its CSVs on the server.
  await expect(page.getByTestId("import-kinds")).toBeVisible();
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

  // Sign out from the account menu (the avatar in the phone header on a
  // phone): the sign-in page, and signed out for real.
  await page.getByTestId(phone ? "account-menu-phone" : "account-menu").click();
  await page.getByTestId("sign-out").click();
  await expect(page).toHaveURL(/\/sign-in$/);
  await expect(page.getByTestId("sign-in-campbooks")).toBeVisible();
  await page.goto("/pipeline");
  await expect(page).toHaveURL(/\/sign-in$/);
});
