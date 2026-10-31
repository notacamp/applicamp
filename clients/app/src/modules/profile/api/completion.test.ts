import { describe, expect, it } from "vitest";

import { parseAction } from "./completion";

describe("a strength item's action", () => {
  it("names where to go", () => {
    expect(parseAction("question:q3")).toEqual({ kind: "question", id: "q3" });
    expect(parseAction("skills")).toEqual({ kind: "skills" });
    expect(parseAction("import")).toEqual({ kind: "import" });
    expect(parseAction("goals")).toEqual({ kind: "goals" });
    expect(parseAction("edit:basics")).toEqual({ kind: "edit", section: "basics" });
    expect(parseAction("edit:languages")).toEqual({ kind: "edit", section: "languages" });
  });

  it("is nothing for a full part or an action it doesn't know", () => {
    expect(parseAction(null)).toBeNull();

    expect(parseAction("edit:pets")).toBeNull();
    expect(parseAction("dance")).toBeNull();
  });
});
