import { findConflicts } from "@notacamp/ui/shortcuts";
import { describe, expect, it } from "vitest";

import { DEFAULT_SHORTCUTS, shortcutMessages } from ".";

describe("the shortcut catalogue", () => {
  it("has no two shortcuts fighting over the same keys", () => {
    expect(findConflicts(DEFAULT_SHORTCUTS)).toEqual([]);
  });

  it("labels every shortcut in every language", () => {
    for (const messages of Object.values(shortcutMessages)) {
      for (const { label } of DEFAULT_SHORTCUTS) {
        const path = label.replace(/^shortcuts:/, "").split(".");
        let value: unknown = messages;
        for (const key of path) {
          value = (value as Record<string, unknown> | undefined)?.[key];
        }
        expect({ label, type: typeof value }).toEqual({ label, type: "string" });
      }
    }
  });
});
