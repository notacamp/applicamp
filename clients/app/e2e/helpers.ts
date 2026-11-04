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
 * How much the app does by itself for the shared demo person (ADR 0010):
 * the click-by-click specs keep "find_only", the spec about preparing by
 * itself "prepare". Specs run one at a time (`workers: 1`).
 */
export async function setAutonomy(page: Page, autonomy: "find_only" | "prepare") {
  expect(await callApi(page, "PUT", "/api/v1/goals", { roles: ["Staff engineer"], autonomy })).toBe(
    200,
  );
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

/** A job board as the API details it, found by its slug (`null` when there is none). */
export function boardBySlug(page: Page, slug: string) {
  return page.evaluate(async (slug) => {
    const list = (await (await fetch("/api/v1/boards")).json()) as {
      boards: { id: string; slug: string }[];
    };
    const board = list.boards.find((candidate) => candidate.slug === slug);
    if (board === undefined) {
      return null;
    }
    return (await (await fetch(`/api/v1/boards/${board.id}`)).json()) as {
      board: { id: string };
      approvable: boolean;
      blocked: string | null;
      report: { proposal?: unknown } | null;
    };
  }, slug);
}

/**
 * Calls the API from the signed-in page like {@link callApi} and gives the
 * answer's status and JSON body.
 */
export function callApiJson(page: Page, method: string, path: string, body?: unknown) {
  return page.evaluate(
    async ({ method, path, body }) => {
      const response = await fetch(path, {
        method,
        headers: { "content-type": "application/json", "X-Applicamp-CSRF": "1" },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      const text = await response.text();
      return { status: response.status, body: (text === "" ? null : JSON.parse(text)) as unknown };
    },
    { method, path, body },
  );
}

/**
 * A one-page PDF saying `lines` (Latin-1 text, one line each, in Helvetica):
 * a CV or a LinkedIn profile saved as PDF, made here so no file is kept.
 */
export function pdfSaying(lines: readonly string[]): Buffer {
  const escape = (line: string) =>
    line
      .replaceAll("\\", String.raw`\\`)
      .replaceAll("(", String.raw`\(`)
      .replaceAll(")", String.raw`\)`);
  const content = [
    "BT",
    "/F1 11 Tf",
    "72 780 Td",
    ...lines.flatMap((line) => [`(${escape(line)}) Tj`, "0 -16 Td"]),
    "ET",
  ].join("\n");
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R " +
      "/Resources << /Font << /F1 5 0 R >> >> >>",
    `<< /Length ${String(Buffer.byteLength(content, "latin1"))} >>\nstream\n${content}\nendstream`,
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
  ];
  let pdf = "%PDF-1.4\n";
  const offsets: number[] = [];
  for (const [index, object] of objects.entries()) {
    offsets.push(Buffer.byteLength(pdf, "latin1"));
    pdf += `${String(index + 1)} 0 obj\n${object}\nendobj\n`;
  }
  const xref = Buffer.byteLength(pdf, "latin1");
  pdf += `xref\n0 ${String(objects.length + 1)}\n0000000000 65535 f \n`;
  pdf += offsets.map((offset) => `${String(offset).padStart(10, "0")} 00000 n \n`).join("");
  pdf += `trailer\n<< /Size ${String(objects.length + 1)} /Root 1 0 R >>\nstartxref\n${String(xref)}\n%%EOF\n`;
  return Buffer.from(pdf, "latin1");
}
