import { expect, test } from "@playwright/test";

import { callApiJson, signIn } from "./helpers";

/** What the owner sees of the job store (GET /api/v1/boards/store). */
interface StoreTotals {
  on: boolean;
  jobs_stored: number;
  new_7d: number;
  sites_read_today: number;
  reads_today: number;
}

// No screen: the stack reads its approved fake board into the job store by
// itself, on its own schedule, and the owner's totals say so.
test("an approved board is read into the job store without a search, and the owner's totals count it", async ({
  page,
}) => {
  await signIn(page);
  await expect
    .poll(
      async () => {
        const { status, body } = await callApiJson(page, "GET", "/api/v1/boards/store");
        expect(status).toBe(200);
        return (body as StoreTotals).jobs_stored;
      },
      { timeout: 120_000, intervals: [2000] },
    )
    .toBeGreaterThan(0);
  const { body } = await callApiJson(page, "GET", "/api/v1/boards/store");
  const totals = body as StoreTotals;
  expect(totals.on).toBe(true);
  expect(totals.new_7d).toBeGreaterThan(0);
  expect(totals.sites_read_today).toBeGreaterThan(0);
  expect(totals.reads_today).toBeGreaterThanOrEqual(totals.sites_read_today);
});
