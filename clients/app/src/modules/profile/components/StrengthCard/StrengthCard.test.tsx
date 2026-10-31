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

describe("the strength card", () => {
  it("shows the score, what is missing and what to do about it", async () => {
    stubApi({
      "GET /api/v1/profile/strength": { body: STRENGTH },
    });
    const onAction = vi.fn();
    const onAbout = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(<StrengthCard onAction={onAction} onAbout={onAbout} />);
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
    // The way to a stronger profile is Tell me about you, not Improve or Grill me.
    expect(screen.getByTestId("strength-about")).toHaveTextContent("Tell me about you");
    expect(screen.queryByTestId("strength-improve")).toBeNull();
    expect(screen.queryByTestId("strength-grill")).toBeNull();
    await user.click(screen.getByTestId("strength-about"));
    expect(onAbout).toHaveBeenCalledTimes(1);
  });

  it("asks nothing of the model by showing the strength", async () => {
    const requests = stubApi({ "GET /api/v1/profile/strength": { body: STRENGTH } });
    renderWithProviders(<StrengthCard onAction={vi.fn()} onAbout={vi.fn()} />);
    expect(await screen.findByTestId("strength-card")).toBeVisible();
    expect(requests.map(({ route }) => route)).toEqual(["GET /api/v1/profile/strength"]);
  });

  it("stays out of the way when the strength cannot be read", async () => {
    stubApi({ "GET /api/v1/profile/strength": { status: 500, body: { code: "internal_error" } } });
    renderWithProviders(<StrengthCard onAction={vi.fn()} onAbout={vi.fn()} />);
    await new Promise((resolve) => {
      setTimeout(resolve, 50);
    });
    expect(screen.queryByTestId("strength-card")).toBeNull();
  });
});
