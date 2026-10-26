import { afterEach, describe, expect, it, vi } from "vitest";

import { PREF_PREFIX, readPref, writePref } from "./prefs";

describe("preferences", () => {
  afterEach(() => {
    globalThis.localStorage.clear();
  });

  it("are kept under the app's prefix", () => {
    writePref("theme", "dark");
    expect(readPref("theme")).toBe("dark");
    expect(globalThis.localStorage.getItem(`${PREF_PREFIX}theme`)).toBe("dark");
    expect(readPref("locale")).toBeNull();
  });

  it("read as empty and drop writes when storage is blocked", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("full");
    });
    expect(readPref("theme")).toBeNull();
    expect(() => {
      writePref("theme", "dark");
    }).not.toThrow();
  });
});
