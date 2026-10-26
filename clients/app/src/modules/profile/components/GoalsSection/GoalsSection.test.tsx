import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi } from "../../../../../test/render";
import { EMPTY_GOALS, type Goals } from "../../api/goals";

import { GoalsSection } from "./GoalsSection";

const ANSWERED: Goals = {
  ...EMPTY_GOALS,
  roles: ["Staff engineer", "Founding engineer"],
  seniority: ["staff"],
  relocate: "within_eu",
  cash_floor: { amount: 90_000, currency: "EUR", period: "year" },
  equity_counts: false,
  languages_for_work: ["en", "pt"],
  autonomy: "prepare",
};

/** A server that keeps the goals, at `version`. */
function fakeGoals(initial: Goals, version: number, failing = false) {
  let answers = initial;
  let current = version;
  const saved: Goals[] = [];
  stubApi({
    "GET /api/v1/goals": () => ({ body: { version: current, answers, updated_at: null } }),
    "PUT /api/v1/goals": async (request) => {
      if (failing) {
        return { status: 500, body: { code: "internal_error" } };
      }
      answers = (await request.json()) as Goals;
      saved.push(answers);
      current += 1;
      return { body: { version: current, answers, updated_at: "2026-10-26T10:00:00Z" } };
    },
  });
  return saved;
}

function renderSection(props: { offer?: boolean } = {}) {
  const onStart = vi.fn();
  const onDismissOffer = vi.fn();
  renderWithProviders(
    <GoalsSection offer={props.offer ?? false} onDismissOffer={onDismissOffer} onStart={onStart} />,
  );
  return { onStart, onDismissOffer };
}

describe("the goals section", () => {
  it("invites you to start, with every answer waiting to be added", async () => {
    fakeGoals(EMPTY_GOALS, 0);
    const user = userEvent.setup();
    const { onStart } = renderSection();

    const properties = await screen.findByTestId("goals-properties");
    expect(properties).toHaveTextContent("Add the roles you want");
    expect(properties).not.toHaveTextContent("Equity");
    expect(screen.getByTestId("goals-status")).toHaveTextContent("0 of 11 questions answered");
    expect(screen.queryByTestId("goals-offer")).toBeNull();
    expect(screen.queryByTestId("goals-continue")).toBeNull();

    await user.click(screen.getByTestId("goals-start"));
    expect(onStart).toHaveBeenCalledWith("roles");
  });

  it("offers the questions after an import, without forcing them", async () => {
    fakeGoals(EMPTY_GOALS, 0);
    const user = userEvent.setup();
    const { onStart, onDismissOffer } = renderSection({ offer: true });

    const offer = await screen.findByTestId("goals-offer");
    expect(offer).toHaveTextContent("Your profile is saved.");
    await user.click(screen.getByTestId("goals-offer-dismiss"));
    expect(onDismissOffer).toHaveBeenCalled();
    await user.click(screen.getByTestId("goals-offer-start"));
    expect(onStart).toHaveBeenCalledWith("roles");
  });

  it("shows each answer in words, and continues where it was left", async () => {
    fakeGoals(ANSWERED, 3);
    const user = userEvent.setup();
    const { onStart } = renderSection({ offer: true });

    const properties = await screen.findByTestId("goals-properties");
    expect(screen.queryByTestId("goals-offer")).toBeNull();
    expect(properties).toHaveTextContent("Staff engineer, Founding engineer");
    expect(properties).toHaveTextContent("Staff");
    expect(properties).toHaveTextContent("Yes, within the EU");
    expect(properties).toHaveTextContent("€90,000 a year");
    expect(properties).toHaveTextContent("No, cash only");
    expect(properties).toHaveTextContent("English, Portuguese");
    expect(properties).toHaveTextContent("Prepare applications");
    expect(screen.getByTestId("goals-status")).toHaveTextContent(
      "7 of 12 questions answered · Version 3",
    );

    await user.click(screen.getByTestId("goals-continue"));
    expect(onStart).toHaveBeenCalledWith("work_mode");
  });

  it("edits the cash floor in place: a new version, and the new floor shows", async () => {
    const saved = fakeGoals(ANSWERED, 3);
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByTestId("goal-cash_floor"));
    const amount = screen.getByTestId("goal-cash_floor-amount");
    expect(amount).toHaveValue("90000");
    await user.clear(amount);
    await user.type(amount, "95000{Enter}");

    expect(await screen.findByTestId("goal-cash_floor")).toHaveTextContent("€95,000 a year");
    expect(saved).toEqual([
      { ...ANSWERED, cash_floor: { amount: 95_000, currency: "EUR", period: "year" } },
    ]);
    expect(screen.getByTestId("goals-status")).toHaveTextContent("Version 4");
  });

  it("edits a list with the interview's own field, and leaves an edit with Esc", async () => {
    const saved = fakeGoals(ANSWERED, 1);
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByTestId("goal-locations"));
    await user.type(screen.getByTestId("goal-locations-input"), "Lisbon");
    await user.click(screen.getByTestId("goal-locations-add"));
    await user.click(screen.getByTestId("goal-locations-save"));
    expect(await screen.findByTestId("goal-locations")).toHaveTextContent("Lisbon");
    expect(saved).toEqual([{ ...ANSWERED, locations: ["Lisbon"] }]);

    await user.click(screen.getByTestId("goal-avoid"));
    await user.type(screen.getByTestId("goal-avoid-input"), "Gambling");
    await user.keyboard("{Escape}");
    expect(screen.getByTestId("goal-avoid")).toHaveTextContent("Add what you want to avoid");
    expect(saved).toHaveLength(1);
  });

  it("changes a choice and refuses an amount that is not one", async () => {
    const saved = fakeGoals(ANSWERED, 1);
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByTestId("goal-relocate"));
    const anywhere = screen
      .getByTestId("goal-relocate-field")
      .querySelector('input[value="anywhere"]');
    if (!(anywhere instanceof HTMLElement)) {
      throw new TypeError("no choice");
    }
    await user.click(anywhere);
    await user.click(screen.getByTestId("goal-relocate-save"));
    expect(await screen.findByTestId("goal-relocate")).toHaveTextContent("Yes, anywhere");
    expect(saved).toEqual([{ ...ANSWERED, relocate: "anywhere" }]);

    await user.click(screen.getByTestId("goal-cash_floor"));
    const amount = screen.getByTestId("goal-cash_floor-amount");
    await user.clear(amount);
    await user.type(amount, "a lot{Enter}");
    expect(screen.getByTestId("goal-cash_floor-amount")).toBeInvalid();
    expect(saved).toHaveLength(1);
    await user.click(screen.getByTestId("goal-cash_floor-cancel"));
    expect(screen.getByTestId("goal-cash_floor")).toHaveTextContent("€90,000 a year");
  });

  it("says when a change could not be saved", async () => {
    fakeGoals(ANSWERED, 1, true);
    const user = userEvent.setup();
    renderSection();

    await user.click(await screen.findByTestId("goal-roles"));
    await user.type(screen.getByTestId("goal-roles-input"), "Designer");
    await user.click(screen.getByTestId("goal-roles-save"));
    expect(await screen.findByTestId("goals-save-error")).toHaveTextContent("couldn’t be saved");
  });

  it("says when the goals cannot be loaded", async () => {
    stubApi({ "GET /api/v1/goals": { status: 500, body: { code: "internal_error" } } });
    renderSection();
    expect(await screen.findByTestId("goals-load-error")).toHaveTextContent("couldn’t be loaded");
  });
});
