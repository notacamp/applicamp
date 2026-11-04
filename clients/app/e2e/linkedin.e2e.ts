import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

import {
  callApi,
  expectAccessible,
  expectNoSidewaysScroll,
  expectPageFillsTheScreen,
  signIn,
} from "./helpers";

// Filmed: CI keeps this spec's videos (the `e2e-recordings` artifact), the
// recording of Your LinkedIn at desktop width and at 375px.
test.use({ video: "on" });

const LINKEDIN_EXPORT = fileURLToPath(new URL("fixtures/linkedin-export.zip", import.meta.url));

/** An invented profile (nobody real), known a little better than the export. */
const PROFILE = {
  basics: { name: "Alex Demo", label: "Backend engineer", profiles: [] },
  work: [
    {
      name: "Example Org",
      position: "Senior Engineer",
      startDate: "2021-03",
      summary: "Runs the billing platform in Rust, with Kafka between its services.",
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
  skills: [{ name: "Rust" }, { name: "Kafka" }],
  languages: [],
  certificates: [],
  projects: [],
};

test("someone has their LinkedIn checked against their profile and goals, keeps a proposal, drops one, edits a flagged one, and marks one done", async ({
  page,
}) => {
  await signIn(page);
  // Both projects share one stack and one person: start from the same
  // profile and goals, with no review left from the other run.
  expect(await callApi(page, "PUT", "/api/v1/profile", PROFILE)).toBe(200);
  expect(await callApi(page, "PUT", "/api/v1/goals", { roles: ["Staff Engineer"] })).toBe(200);
  expect(await callApi(page, "DELETE", "/api/v1/profile/linkedin")).toBe(204);

  // Profile & goals: "Your LinkedIn" is the way in.
  await page.goto("/profile");
  await expect(page.getByTestId("linkedin-section")).toBeVisible();
  await page.getByTestId("linkedin-open").click();
  await expect(page).toHaveURL(/\/profile\/linkedin$/);
  await expect(page.getByTestId("linkedin-upload")).toBeVisible();
  await expectPageFillsTheScreen(page, "linkedin");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // The export is read on the server; one model call writes the review.
  await page.getByTestId("linkedin-file").setInputFiles(LINKEDIN_EXPORT);
  await page.getByTestId("linkedin-submit").click();
  await expect(page.getByTestId("linkedin-review")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId("linkedin-overall")).toContainText("Staff Engineer");
  await expect(page.getByTestId("linkedin-state-f1")).toHaveAttribute(
    "data-state",
    "could_be_better",
  );
  await expect(page.getByTestId("linkedin-finding-f1")).toContainText("On LinkedIn now:");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // A clean headline is kept: it goes on the list with its text to copy.
  await page.getByTestId("linkedin-keep-p1").click();
  await expect(page.getByTestId("linkedin-proposal-p1")).toHaveAttribute("data-status", "kept");
  await expect(page.getByTestId("linkedin-todo-p1")).toBeVisible();
  await expect(page.getByTestId("linkedin-copy-p1")).toContainText("Copy");

  // A buzzword nobody wrote: Keep is off; it is dropped.
  await expect(page.getByTestId("linkedin-flags-p2")).toContainText("nobody wrote about you");
  await expect(page.getByTestId("linkedin-keep-p2")).toBeDisabled();
  await page.getByTestId("linkedin-drop-p2").click();
  await expect(page.getByTestId("linkedin-proposal-p2")).toHaveAttribute("data-status", "dropped");

  // A tool no source names: edited out, checked again, then kept.
  await expect(page.getByTestId("linkedin-flags-p3")).toContainText("Kubernetes");
  await expect(page.getByTestId("linkedin-keep-p3")).toBeDisabled();
  const before = (await page.getByTestId("linkedin-proposal-text-p3").textContent()) ?? "";
  await page.getByTestId("linkedin-edit-p3").click();
  await page
    .getByTestId("linkedin-edit-text-p3")
    .fill(before.replace(" Also runs Kubernetes.", ""));
  await page.getByTestId("linkedin-save-p3").click();
  await expect(page.getByTestId("linkedin-flags-p3")).toHaveCount(0);
  await expect(page.getByTestId("linkedin-keep-p3")).toBeEnabled();
  await page.getByTestId("linkedin-keep-p3").click();
  await expect(page.getByTestId("linkedin-proposal-p3")).toHaveAttribute("data-status", "kept");

  // Changed on LinkedIn: done, and it can be undone.
  await page.getByTestId("linkedin-done-p1").click();
  await expect(page.getByTestId("linkedin-todo-p1")).toHaveAttribute("data-status", "done");
  await expectNoSidewaysScroll(page);
  await expectAccessible(page);

  // Profile & goals says where things stand, and the review survives a reload.
  await page.getByTestId("linkedin-back").click();
  await expect(page).toHaveURL(/\/profile$/);
  await expect(page.getByTestId("linkedin-section-counts")).toContainText("1 done");
  await page.goto("/profile/linkedin");
  await expect(page.getByTestId("linkedin-todo-p1")).toHaveAttribute("data-status", "done");
});
