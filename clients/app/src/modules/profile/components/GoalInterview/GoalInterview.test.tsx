import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi } from "../../../../../test/render";
import { EMPTY_GOALS, type Goals } from "../../api/goals";
import type { Question } from "../../api/questions";

import { GoalInterview } from "./GoalInterview";

/** The interview as the route drives it, with `?step=` as state. */
function Interview({
  start,
  onLeave,
}: {
  start?: Question | undefined;
  onLeave: (reason: "later" | "done") => void;
}) {
  const [question, setQuestion] = useState(start);
  return <GoalInterview question={question} onQuestion={setQuestion} onLeave={onLeave} />;
}

/** A server that keeps the goals: what was saved, in order, and the version. */
function fakeGoals(initial: Goals = EMPTY_GOALS, failing = false) {
  const saved: Goals[] = [];
  let version = initial === EMPTY_GOALS ? 0 : 1;
  let answers = initial;
  const requests = stubApi({
    "GET /api/v1/goals": () => ({ body: { version, answers, updated_at: null } }),
    "PUT /api/v1/goals": async (request) => {
      if (failing) {
        return { status: 500, body: { code: "internal_error", status: 500, title: "Oops" } };
      }
      answers = (await request.json()) as Goals;
      saved.push(answers);
      version += 1;
      return { body: { version, answers, updated_at: "2026-10-26T10:00:00Z" } };
    },
  });
  return { saved, puts: () => requests.filter(({ route }) => route.startsWith("PUT")).length };
}

/** The input of a choice (a radio or a checkbox) in a question's field. */
function choice(question: string, value: string): HTMLElement {
  const input = screen
    .getByTestId(`goal-${question}-field`)
    .querySelector(`input[value="${value}"]`);
  if (!(input instanceof HTMLElement)) {
    throw new TypeError(`No choice ${value} in ${question}`);
  }
  return input;
}

describe("the goals interview", () => {
  it("asks one question at a time: Shift+Enter adds an item, Enter saves and goes on", async () => {
    const { saved } = fakeGoals();
    const user = userEvent.setup();
    renderWithProviders(<Interview onLeave={vi.fn()} />);

    const first = await screen.findByTestId("interview-roles");
    expect(first).toHaveTextContent("Question 1 of 11");
    expect(first).toHaveTextContent("What roles do you want next?");
    expect(first).toHaveTextContent("Every question is optional.");
    expect(screen.queryByTestId("interview-back")).toBeNull();
    expect(screen.queryByTestId("interview-seniority")).toBeNull();

    await user.type(screen.getByTestId("goal-roles-input"), "Staff engineer");
    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(screen.getByTestId("goal-roles-item-0")).toHaveTextContent("Staff engineer");
    expect(screen.getByTestId("goal-roles-input")).toHaveValue("");

    // Enter adds what is typed, saves, and shows the next question.
    await user.type(screen.getByTestId("goal-roles-input"), "Founding engineer{Enter}");
    expect(await screen.findByTestId("interview-seniority")).toHaveTextContent("Question 2 of 11");
    expect(saved).toEqual([{ ...EMPTY_GOALS, roles: ["Staff engineer", "Founding engineer"] }]);
  });

  it("limits a list, drops repeats and removes an item", async () => {
    fakeGoals();
    const user = userEvent.setup();
    renderWithProviders(<Interview onLeave={vi.fn()} />);

    const input = await screen.findByTestId("goal-roles-input");
    for (const role of ["a", "A", "b", "c", "d", "e", "f"]) {
      // Once the list is full, the field and Add wait: these do nothing.
      await user.type(input, role);
      await user.click(screen.getByTestId("goal-roles-add"));
    }
    expect(screen.getByTestId("goal-roles-item-4")).toHaveTextContent("e");
    expect(screen.queryByTestId("goal-roles-item-5")).toBeNull();
    expect(input).toBeDisabled();

    await user.click(screen.getByTestId("goal-roles-remove-0"));
    expect(screen.getByTestId("goal-roles-item-0")).toHaveTextContent("b");
    expect(input).toBeEnabled();
  });

  it("skips without saving, and goes back to the question before", async () => {
    const { puts } = fakeGoals();
    const user = userEvent.setup();
    renderWithProviders(<Interview start="seniority" onLeave={vi.fn()} />);

    expect(await screen.findByTestId("interview-seniority")).toBeInTheDocument();
    await user.click(choice("seniority", "senior"));
    await user.click(screen.getByTestId("interview-skip"));
    expect(await screen.findByTestId("interview-work_mode")).toBeInTheDocument();
    await user.click(screen.getByTestId("interview-back"));
    expect(await screen.findByTestId("interview-seniority")).toBeInTheDocument();
    expect(choice("seniority", "senior")).not.toBeChecked();
    expect(puts()).toBe(0);
  });

  it("saves a choice with Next and keeps it when coming back", async () => {
    const { saved } = fakeGoals();
    const user = userEvent.setup();
    renderWithProviders(<Interview start="seniority" onLeave={vi.fn()} />);

    await screen.findByTestId("interview-seniority");
    await user.click(choice("seniority", "senior"));
    await user.click(choice("seniority", "staff"));
    await user.click(screen.getByTestId("interview-next"));
    expect(await screen.findByTestId("interview-work_mode")).toBeInTheDocument();
    expect(saved).toEqual([{ ...EMPTY_GOALS, seniority: ["senior", "staff"] }]);

    await user.click(screen.getByTestId("interview-back"));
    await screen.findByTestId("interview-seniority");
    expect(choice("seniority", "staff")).toBeChecked();
  });

  it("leaves with Finish later, keeping what was typed", async () => {
    const { saved } = fakeGoals();
    const user = userEvent.setup();
    const onLeave = vi.fn();
    renderWithProviders(<Interview onLeave={onLeave} />);

    await user.type(await screen.findByTestId("goal-roles-input"), "Designer");
    await user.click(screen.getByTestId("interview-later"));
    await vi.waitFor(() => {
      expect(onLeave).toHaveBeenCalledWith("later");
    });
    expect(saved).toEqual([{ ...EMPTY_GOALS, roles: ["Designer"] }]);
  });

  it("leaves with Esc, saving nothing when nothing changed", async () => {
    const { puts } = fakeGoals();
    const user = userEvent.setup();
    const onLeave = vi.fn();
    renderWithProviders(<Interview onLeave={onLeave} />);

    await screen.findByTestId("interview-roles");
    await user.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalledWith("later");
    expect(puts()).toBe(0);
  });

  it("takes a cash floor in any currency, then asks about equity", async () => {
    const { saved } = fakeGoals();
    const user = userEvent.setup();
    renderWithProviders(<Interview start="cash_floor" onLeave={vi.fn()} />);

    const amount = await screen.findByTestId("goal-cash_floor-amount");
    expect(screen.getByTestId("goal-cash_floor-currency")).toHaveValue("EUR");
    await user.type(amount, "lots");
    await user.click(screen.getByTestId("interview-next"));
    expect(screen.getByTestId("interview-cash_floor")).toHaveTextContent("Enter a whole amount");
    expect(saved).toEqual([]);

    await user.clear(amount);
    await user.type(amount, "95,000");
    await user.selectOptions(screen.getByTestId("goal-cash_floor-currency"), "USD");
    await user.click(screen.getByTestId("interview-next"));
    expect(await screen.findByTestId("interview-equity_counts")).toBeInTheDocument();
    expect(saved).toEqual([
      { ...EMPTY_GOALS, cash_floor: { amount: 95_000, currency: "USD", period: "year" } },
    ]);
  });

  it("skips the cash floor and goes past the equity question", async () => {
    fakeGoals();
    const user = userEvent.setup();
    renderWithProviders(<Interview start="cash_floor" onLeave={vi.fn()} />);

    await screen.findByTestId("interview-cash_floor");
    await user.click(screen.getByTestId("interview-skip"));
    expect(await screen.findByTestId("interview-company_stage")).toBeInTheDocument();
  });

  it("ends after the last question", async () => {
    const { saved } = fakeGoals();
    const user = userEvent.setup();
    const onLeave = vi.fn();
    renderWithProviders(<Interview start="autonomy" onLeave={onLeave} />);

    const page = await screen.findByTestId("interview-autonomy");
    expect(page).toHaveTextContent("Applying and emailing always wait for your approval");
    expect(screen.getByTestId("interview-next")).toHaveTextContent("Done");
    await user.click(choice("autonomy", "prepare"));
    await user.click(screen.getByTestId("interview-next"));
    await vi.waitFor(() => {
      expect(onLeave).toHaveBeenCalledWith("done");
    });
    expect(saved).toEqual([{ ...EMPTY_GOALS, autonomy: "prepare" }]);
  });

  it("says when an answer could not be saved, and stays on the question", async () => {
    fakeGoals(EMPTY_GOALS, true);
    const user = userEvent.setup();
    renderWithProviders(<Interview start="relocate" onLeave={vi.fn()} />);

    await screen.findByTestId("interview-relocate");
    await user.click(choice("relocate", "anywhere"));
    await user.click(screen.getByTestId("interview-next"));
    expect(await screen.findByTestId("interview-save-error")).toHaveTextContent(
      "couldn’t be saved",
    );
    expect(screen.getByTestId("interview-relocate")).toBeInTheDocument();
  });

  it("says when the goals cannot be loaded, and still lets you leave", async () => {
    stubApi({ "GET /api/v1/goals": { status: 500, body: { code: "internal_error" } } });
    const user = userEvent.setup();
    const onLeave = vi.fn();
    renderWithProviders(<Interview onLeave={onLeave} />);

    expect(await screen.findByTestId("interview-load-error")).toHaveTextContent(
      "couldn’t be loaded",
    );
    await user.click(screen.getByTestId("interview-later"));
    expect(onLeave).toHaveBeenCalledWith("later");
  });
});
