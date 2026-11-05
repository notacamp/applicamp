import { describe, expect, it } from "vitest";

import { placeOf } from "./links";

describe("the place of an address", () => {
  it("is the place itself, or the one the page is reached from", () => {
    expect(placeOf("/")).toBe("today");
    expect(placeOf("/pipeline")).toBe("pipeline");
    expect(placeOf("/pipeline/abc/cv")).toBe("pipeline");
    expect(placeOf("/cvs")).toBe("pipeline");
    expect(placeOf("/profile")).toBe("profile");
    expect(placeOf("/profile/complete")).toBe("profile");
    expect(placeOf("/boards")).toBe("profile");
    expect(placeOf("/companies")).toBe("profile");
    expect(placeOf("/agents")).toBe("today");
    expect(placeOf("/nowhere")).toBe("today");
  });
});
