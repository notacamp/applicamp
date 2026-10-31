import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi } from "../../../../../test/render";

import { StrengthCard } from "./StrengthCard";

const item = (
  key: string,
  state: string,
  extra: { have?: number; of?: number; action?: string | null } = {},
) => ({
  key,
  state,
  earned: state === "ok" ? 6 : 0,
  weight: 6,
  have: extra.have ?? 0,
  of: extra.of ?? 0,
  action: extra.action ?? null,
});

const STRENGTH = {
  score: 45,
  strong: false,
  questions_left: 9,
  suggested_skills: 11,
  items: [
    item("headline", "missing", { action: "question:q2" }),
    item("summary", "missing", { action: "edit:basics" }),
    item("achievements", "partial", { have: 2, of: 3, action: "question:q1" }),
    item("skills", "missing", { action: "skills" }),
    item("education", "ok", { have: 1 }),
    item("languages", "ok", { have: 2 }),
    item("links", "missing", { action: "edit:basics" }),
    item("positions", "ok", { have: 13, of: 3 }),
    item("goals", "ok", { have: 12, of: 12 }),
  ],
};

const questions = (n: number) => ({
  status: "ready",
  reason: null,
  score: 45,
  total: n,
  answered: 0,
  questions: Array.from({ length: n }, (_, i) => ({
    id: `q${String(i + 1)}`,
    section: "basics.label",
    context: null,
    question: "Q?",
    why: "W",
    expected_gain: 6,
  })),
});

describe("the strength card", () => {
  it("shows the score, what is missing and what to do about it", async () => {
    stubApi({
      "GET /api/v1/profile/strength": { body: STRENGTH },
      "GET /api/v1/profile/questions": { body: questions(9) },
    });
    const onAction = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(<StrengthCard onAction={onAction} />);
    expect(await screen.findByTestId("strength-score")).toHaveTextContent("45%");
    expect(screen.getByTestId("strength-card")).toHaveTextContent("35 points to a strong profile");
    expect(screen.getByTestId("strength-card")).toHaveTextContent(
      "None listed · 11 found in your roles",
    );
    expect(screen.getByTestId("strength-card")).toHaveTextContent(
      "2 of 3 recent roles have results",
    );
    // A full part has nothing to do.
    expect(screen.queryByTestId("strength-education-action")).toBeNull();
    expect(screen.getByTestId("strength-achievements-action")).toHaveTextContent("Improve");
    expect(screen.getByTestId("strength-headline-action")).toHaveTextContent("Add");

    await user.click(screen.getByTestId("strength-headline-action"));
    expect(onAction).toHaveBeenLastCalledWith({ kind: "question", id: "q2" });
    await user.click(screen.getByTestId("strength-skills-action"));
    expect(onAction).toHaveBeenLastCalledWith({ kind: "skills" });
    await user.click(screen.getByTestId("strength-summary-action"));
    expect(onAction).toHaveBeenLastCalledWith({ kind: "edit", section: "basics" });
    expect(await screen.findByTestId("strength-improve")).toHaveTextContent(
      "Improve my profile · 9 questions",
    );
    await user.click(screen.getByTestId("strength-improve"));
    expect(onAction).toHaveBeenLastCalledWith({ kind: "improve" });
  });

  it("is quiet when the profile is strong and nothing is left to ask", async () => {
    stubApi({
      "GET /api/v1/profile/strength": {
        body: {
          ...STRENGTH,
          score: 90,
          strong: true,
          items: STRENGTH.items.map((i) => ({ ...i, state: "ok" })),
        },
      },
      "GET /api/v1/profile/questions": { body: questions(0) },
    });
    renderWithProviders(<StrengthCard onAction={vi.fn()} />);
    expect(await screen.findByTestId("strength-card")).toHaveTextContent("Strong.");
    expect(screen.queryByTestId("strength-improve")).toBeNull();
  });

  it("works without the questions: the checklist stands on its own", async () => {
    stubApi({
      "GET /api/v1/profile/strength": { body: STRENGTH },
      "GET /api/v1/profile/questions": {
        body: { ...questions(0), status: "unavailable", reason: "ai_unavailable" },
      },
    });
    renderWithProviders(<StrengthCard onAction={vi.fn()} />);
    expect(await screen.findByTestId("strength-improve")).toHaveTextContent("Improve my profile");
  });

  it("stays out of the way when the strength cannot be read", async () => {
    stubApi({ "GET /api/v1/profile/strength": { status: 500, body: { code: "internal_error" } } });
    renderWithProviders(<StrengthCard onAction={vi.fn()} />);
    await new Promise((resolve) => {
      setTimeout(resolve, 50);
    });
    expect(screen.queryByTestId("strength-card")).toBeNull();
  });
});
