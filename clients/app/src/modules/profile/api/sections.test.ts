import { describe, expect, it } from "vitest";

import { EMPTY_PROFILE, isEmptyProfile, type Profile } from "./profile";
import {
  compareBasics,
  compareList,
  entryOf,
  isSameEntry,
  problemsOf,
  valuesOf,
  withList,
} from "./sections";

const CURRENT: Profile = {
  ...EMPTY_PROFILE,
  basics: { profiles: [], label: "Engineer", name: "Alex Demo" },
  work: [
    { name: "Example Org", position: "Principal Engineer", startDate: "2021-03", highlights: [] },
  ],
  skills: [{ name: "Rust" }],
};

describe("the profile's lists", () => {
  it("compares a draft with the profile, entry by entry", () => {
    const draft: Profile = {
      ...EMPTY_PROFILE,
      basics: { profiles: [], label: "Staff engineer", name: "alex  demo", location: "Lisbon" },
      work: [
        // The same position, its title edited by hand since.
        { name: "Example Org", position: "Senior Engineer", startDate: "2021-03", highlights: [] },
        { name: "Sample Labs", position: "Engineer", startDate: "2017-06", highlights: [] },
      ],
      skills: [{ name: "rust" }, { name: "SQL" }],
    };
    expect(compareList("work", CURRENT, draft).map(({ change }) => change)).toEqual([
      "changed",
      "new",
    ]);
    expect(compareList("skills", CURRENT, draft).map(({ change }) => change)).toEqual([
      "same",
      "new",
    ]);
    expect(compareBasics("label", CURRENT.basics, draft.basics)).toBe("changed");
    expect(compareBasics("name", CURRENT.basics, draft.basics)).toBe("same");
    expect(compareBasics("location", CURRENT.basics, draft.basics)).toBe("new");
    expect(compareBasics("email", CURRENT.basics, draft.basics)).toBeUndefined();
  });

  it("knows an entry by the server's rule", () => {
    const work = { name: "Example Org", position: "Engineer", startDate: "2020", highlights: [] };
    expect(isSameEntry("work", work, { ...work, startDate: "2019" })).toBe(true);
    expect(isSameEntry("work", work, { ...work, position: "Lead" })).toBe(true);
    expect(isSameEntry("work", work, { ...work, position: "Lead", startDate: "2019" })).toBe(false);
    expect(
      isSameEntry(
        "education",
        { institution: "Sample University", studyType: "MSc" },
        { institution: "sample university", studyType: "MSc" },
      ),
    ).toBe(true);
    expect(isSameEntry("languages", { language: "English" }, { language: "French" })).toBe(false);
  });

  it("turns entries into form values and back", () => {
    const work = {
      name: "Example Org",
      position: "Engineer",
      highlights: ["Shipped the ledger", "Halved the bill"],
    };
    const values = valuesOf("work", work);
    expect(values.highlights).toBe("Shipped the ledger\nHalved the bill");
    expect(values.startDate).toBe("");
    expect(entryOf("work", { ...values, highlights: " One \n\n Two " })).toEqual({
      name: "Example Org",
      position: "Engineer",
      highlights: ["One", "Two"],
    });
    expect(valuesOf("skills", undefined)).toEqual({ name: "", level: "" });
    expect(entryOf("skills", { name: " Go ", level: "" })).toEqual({ name: "Go" });
    expect(entryOf("work", { name: "Example Org" })).toEqual({
      name: "Example Org",
      highlights: [],
    });
  });

  it("finds what a form is missing", () => {
    expect(problemsOf("skills", { name: " " })).toEqual({ name: "required" });
    expect(problemsOf("work", { startDate: "March 2020" })).toEqual({
      startDate: "date",
      position: "required",
    });
    expect(problemsOf("work", { name: "Example Org", startDate: "2020-03" })).toEqual({});
  });

  it("knows an empty profile and replaces a list", () => {
    expect(isEmptyProfile(EMPTY_PROFILE)).toBe(true);
    expect(isEmptyProfile(CURRENT)).toBe(false);
    expect(isEmptyProfile({ ...EMPTY_PROFILE, basics: { profiles: [], name: " " } })).toBe(true);
    const next = withList(CURRENT, "skills", [{ name: "Go" }]);
    expect(next.skills).toEqual([{ name: "Go" }]);
    expect(CURRENT.skills).toEqual([{ name: "Rust" }]);
  });
});
