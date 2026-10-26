import { describe, expect, it } from "vitest";

import css from "./Page.module.css?raw";

/** The declarations of the base `.page` rule: the first one, before any media query. */
function basePageRule(): string {
  return /\.page\s*\{([^}]*)\}/.exec(css)?.[1] ?? "";
}

// jsdom has no layout, so this guards the rule itself; the phone e2e project
// checks the result at 375px.
describe("the page's size in the Shell", () => {
  it("never starts from a zero basis on a phone, where the Shell wraps", () => {
    const page = basePageRule();
    expect(page).toContain("flex: 999 1 var(--reader-min)");
    expect(page).not.toMatch(/flex:\s*\d+\s+\d+\s+0\b/);
  });

  it("takes what the sidebar leaves only once the Shell is one row", () => {
    expect(css).toMatch(/@media \(width >= 64rem\)\s*\{\s*\.page\s*\{\s*flex-basis: 0;/);
  });
});
