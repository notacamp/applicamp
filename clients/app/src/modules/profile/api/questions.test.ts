import { describe, expect, it } from "vitest";

import { EMPTY_GOALS, type Goals } from "./goals";
import {
  answeredCount,
  firstOpen,
  floorOf,
  isAnswered,
  neighbour,
  pendingOf,
  QUESTIONS,
  questionsFor,
  sameAnswers,
  settle,
  withFloor,
  withItems,
} from "./questions";

const FLOOR = { amount: 90_000, currency: "EUR", period: "year" } as const;

describe("the goals questions", () => {
  it("asks twelve questions, and equity only once there is a cash floor", () => {
    expect(QUESTIONS).toHaveLength(12);
    expect(questionsFor(EMPTY_GOALS)).toHaveLength(11);
    expect(questionsFor(EMPTY_GOALS)).not.toContain("equity_counts");
    expect(questionsFor({ ...EMPTY_GOALS, cash_floor: FLOOR })).toContain("equity_counts");
  });

  it("counts an answer when a list has items or a choice is made, even a no", () => {
    expect(isAnswered(EMPTY_GOALS, "roles")).toBe(false);
    expect(isAnswered({ ...EMPTY_GOALS, roles: ["Designer"] }, "roles")).toBe(true);
    expect(isAnswered({ ...EMPTY_GOALS, equity_counts: false }, "equity_counts")).toBe(true);
    expect(isAnswered({ ...EMPTY_GOALS, relocate: null }, "relocate")).toBe(false);
    expect(answeredCount({ ...EMPTY_GOALS, roles: ["a"], relocate: "no" })).toBe(2);
  });

  it("goes on from the first question not yet answered", () => {
    expect(firstOpen(EMPTY_GOALS)).toBe("roles");
    expect(firstOpen({ ...EMPTY_GOALS, roles: ["a"], seniority: ["senior"] })).toBe("work_mode");
    const everything: Goals = {
      roles: ["a"],
      seniority: ["mid"],
      work_mode: ["remote"],
      locations: ["Lisbon"],
      relocate: "no",
      cash_floor: FLOOR,
      equity_counts: true,
      company_stage: ["early"],
      avoid: ["x"],
      must_have: ["y"],
      languages_for_work: ["en"],
      autonomy: "prepare",
    };
    expect(firstOpen(everything)).toBe("roles");
  });

  it("moves between questions, jumping over equity while it is not asked", () => {
    expect(neighbour(EMPTY_GOALS, "roles", "previous")).toBeUndefined();
    expect(neighbour(EMPTY_GOALS, "roles", "next")).toBe("seniority");
    expect(neighbour(EMPTY_GOALS, "cash_floor", "next")).toBe("company_stage");
    expect(neighbour(EMPTY_GOALS, "company_stage", "previous")).toBe("cash_floor");
    const priced = { ...EMPTY_GOALS, cash_floor: FLOOR };
    expect(neighbour(priced, "cash_floor", "next")).toBe("equity_counts");
    expect(neighbour(priced, "autonomy", "next")).toBeUndefined();
  });

  it("reads the cash floor typed: empty is none, grouped thousands are fine, junk is invalid", () => {
    const typed = (amount: string) => floorOf({ ...pendingOf(EMPTY_GOALS), amount });
    expect(typed("")).toBeNull();
    expect(typed("  ")).toBeNull();
    expect(typed("90000")).toEqual({ amount: 90_000, currency: "EUR", period: "year" });
    expect(typed("90,000")).toEqual(expect.objectContaining({ amount: 90_000 }));
    expect(typed("90.000")).toEqual(expect.objectContaining({ amount: 90_000 }));
    expect(typed("1 250 000")).toEqual(expect.objectContaining({ amount: 1_250_000 }));
    for (const junk of ["abc", "0", "90.5", "-5", "100000001", "9,00"]) {
      expect(typed(junk), junk).toBe("invalid");
    }
  });

  it("starts the pending state from the cash floor, else EUR a year", () => {
    expect(pendingOf(EMPTY_GOALS)).toEqual({
      draft: "",
      amount: "",
      currency: "EUR",
      period: "year",
    });
    expect(
      pendingOf({ ...EMPTY_GOALS, cash_floor: { amount: 5000, currency: "GBP", period: "month" } }),
    ).toEqual({ draft: "", amount: "5000", currency: "GBP", period: "month" });
  });

  it("settles what is typed into the answers", () => {
    const base = pendingOf(EMPTY_GOALS);
    expect(settle("roles", EMPTY_GOALS, { ...base, draft: " Designer " })).toEqual({
      ...EMPTY_GOALS,
      roles: ["Designer"],
    });
    expect(settle("cash_floor", EMPTY_GOALS, { ...base, amount: "x" })).toBe("invalid");
    expect(settle("cash_floor", EMPTY_GOALS, { ...base, amount: "100" })).toEqual({
      ...EMPTY_GOALS,
      cash_floor: { amount: 100, currency: "EUR", period: "year" },
    });
    expect(settle("cash_floor", { ...EMPTY_GOALS, cash_floor: FLOOR }, base)).toEqual(EMPTY_GOALS);
    expect(settle("autonomy", EMPTY_GOALS, base)).toEqual(EMPTY_GOALS);
  });

  it("changes lists and the floor without touching the rest, and compares answers by value", () => {
    const a = withItems(EMPTY_GOALS, "must_have", ["x"]);
    expect(a.must_have).toEqual(["x"]);
    expect(withItems(a, "roles", ["r"]).must_have).toEqual(["x"]);
    expect(withItems(a, "locations", ["l"]).locations).toEqual(["l"]);
    expect(withItems(a, "avoid", ["v"]).avoid).toEqual(["v"]);
    expect(withFloor(a, FLOOR).cash_floor).toEqual(FLOOR);
    expect(withFloor(withFloor(a, FLOOR), null)).toEqual(a);
    expect(sameAnswers(a, { ...a })).toBe(true);
    expect(sameAnswers({ ...EMPTY_GOALS, relocate: null }, EMPTY_GOALS)).toBe(true);
    expect(sameAnswers(withFloor(a, FLOOR), withFloor(a, { ...FLOOR, amount: 95_000 }))).toBe(
      false,
    );
  });
});
