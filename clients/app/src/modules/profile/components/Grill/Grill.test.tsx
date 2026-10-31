import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { GrillPage } from "./GrillPage";

const ROLE = {
  position: "Senior Engineer",
  company: "Example Org",
  start_date: "2021-03",
  end_date: null,
};

const AREAS = [
  "context",
  "your_part",
  "stack_scale",
  "results",
  "hard_problems",
  "people",
  "stories",
];

const list = (overrides: Record<string, unknown> = {}) => ({
  roles: [
    { position: 0, role: ROLE, status: "not_started", covered: 0, kept: 0 },
    {
      position: 1,
      role: {
        position: "Engineer",
        company: "Sample Labs",
        start_date: "2017-06",
        end_date: "2021-02",
      },
      status: "in_progress",
      covered: 2,
      kept: 1,
    },
  ],
  calls_today: 3,
  daily_cap: 150,
  ...overrides,
});

const turn = (seq: number, kind: string, text: string, extra: Record<string, unknown> = {}) => ({
  seq,
  kind,
  text,
  push: false,
  area: "context",
  dropped: 0,
  ...extra,
});

interface Proposal {
  id: string;
  text: string;
  quote: string;
  area: string;
  turn_seq: number;
  status: string;
  expires_at: string;
}

const FACT: Proposal = {
  id: "f1",
  text: "Took 7 months",
  quote: "It took 7 months",
  area: "results",
  turn_seq: 2,
  status: "ready",
  expires_at: "2026-10-31T10:00:00Z",
};

interface View {
  position: number;
  role: typeof ROLE;
  status: string;
  coverage: { area: string; level: string }[];
  covered: number;
  turns: ReturnType<typeof turn>[];
  question: { text: string; area: string; push: boolean } | null;
  proposals: Proposal[];
  kept: number;
  calls_today: number;
  daily_cap: number;
}

function view(overrides: Partial<View> = {}, ok = 0): View {
  return {
    position: 0,
    role: ROLE,
    status: "in_progress",
    coverage: AREAS.map((area, i) => ({ area, level: i < ok ? "ok" : "none" })),
    covered: ok,
    turns: [turn(1, "question", "What did Example Org sell?")],
    question: { text: "What did Example Org sell?", area: "context", push: false },
    proposals: [],
    kept: 0,
    calls_today: 3,
    daily_cap: 150,
    ...overrides,
  };
}

const routes = (extra: Record<string, Stub> = {}) =>
  stubApi({
    "GET /api/v1/profile/grill": { body: list() },
    "GET /api/v1/profile/grill/0": { body: view() },
    ...extra,
  });

function renderPage({
  position,
  onPosition = vi.fn(),
}: { position?: number; onPosition?: (position: number) => void } = {}) {
  return renderWithProviders(
    <GrillPage
      position={position}
      onPosition={onPosition}
      onBack={vi.fn()}
      spend={<p data-testid="spend">AI this month: €0.02 of €5</p>}
    />,
  );
}

describe("Grill me", () => {
  it("lists the roles with how far each is, and opens one", async () => {
    routes();
    const onPosition = vi.fn();
    const user = userEvent.setup();
    renderPage({ onPosition });
    const first = await screen.findByTestId("grill-role-0");
    expect(first).toHaveTextContent("Senior Engineer");
    expect(first).toHaveTextContent("Example Org · 2021–now · not started");
    expect(screen.getByTestId("grill-role-1")).toHaveTextContent("2017–2021 · in progress · 2/7");
    expect(screen.getByTestId("grill-pick")).toHaveTextContent("about €0.001");
    expect(screen.getByTestId("spend")).toBeVisible();
    await user.click(screen.getByTestId("grill-role-1"));
    expect(onPosition).toHaveBeenCalledWith(1);
  });

  it("says there is nothing to talk about without roles", async () => {
    routes({ "GET /api/v1/profile/grill": { body: list({ roles: [] }) } });
    renderPage();
    expect(await screen.findByTestId("grill-no-roles")).toHaveTextContent("No roles to talk about");
  });

  it("starts a role only when asked, in the person's language", async () => {
    const requests = routes({
      "GET /api/v1/profile/grill/0": {
        body: view({ turns: [], question: null, status: "not_started" }),
      },
      "POST /api/v1/profile/grill/0/start": { body: view() },
    });
    const user = userEvent.setup();
    renderPage({ position: 0 });
    await user.click(await screen.findByTestId("grill-start"));
    expect(await screen.findByTestId("grill-turn-1")).toHaveTextContent(
      "What did Example Org sell?",
    );
    expect(requests.map(({ route }) => route)).toContain("POST /api/v1/profile/grill/0/start");
    expect(screen.getByTestId("grill-cost")).toHaveTextContent("Questions today: 3 of 150");
    expect(screen.getByTestId("grill-coverage-count")).toHaveTextContent("0/7");
  });

  it("answers on Enter, keeps the text on Shift+Enter, and shows a push-back as one", async () => {
    let body: unknown;
    routes({
      "POST /api/v1/profile/grill/0/answer": async (request) => {
        body = await request.json();
        return {
          body: view({
            turns: [
              turn(1, "question", "What did Example Org sell?"),
              turn(2, "answer", "It went well."),
              turn(3, "question", "Who decided, and how long did it take?", { push: true }),
            ],
            question: {
              text: "Who decided, and how long did it take?",
              area: "context",
              push: true,
            },
          }),
        };
      },
    });
    const user = userEvent.setup();
    renderPage({ position: 0 });
    const answer = await screen.findByTestId("grill-answer");
    expect(screen.getByTestId("grill-send")).toBeDisabled();
    await user.type(answer, "It went{Shift>}{Enter}{/Shift}well.{Enter}");
    await waitFor(() => {
      expect(body).toEqual({ text: "It went\nwell." });
    });
    expect(await screen.findByTestId("grill-turn-3")).toHaveTextContent("Pushing back");
    expect(screen.getByTestId("grill-turn-2")).toHaveTextContent("You");
    expect(answer).toHaveValue("");
  });

  it("shows the facts taken from an answer with the person's words and keeps one", async () => {
    let kept: unknown;
    const taken = view(
      {
        turns: [
          turn(1, "question", "How long?"),
          turn(2, "answer", "It took 7 months.", { dropped: 2 }),
          turn(3, "question", "And the team?"),
        ],
        question: { text: "And the team?", area: "people", push: false },
        proposals: [FACT, { ...FACT, id: "f2", text: "2 helped", quote: "2 helped" }],
      },
      1,
    );
    const second = taken.proposals.slice(1);
    routes({
      "GET /api/v1/profile/grill/0": { body: taken },
      "POST /api/v1/profile/grill/proposals/f1/keep": async (request) => {
        kept = await request.json();
        return {
          body: { ...taken, proposals: [{ ...FACT, status: "kept" }, ...second], kept: 1 },
        };
      },
    });
    const user = userEvent.setup();
    renderPage({ position: 0 });
    const block = await screen.findByTestId("grill-facts-2");
    expect(block).toHaveTextContent("Facts taken from your answer");
    expect(block).toHaveTextContent("Your words: It took 7 months");
    expect(block).toHaveTextContent("Saved to Senior Engineer · Example Org in your profile");
    expect(screen.getByTestId("grill-left-out-2")).toHaveTextContent("2 lines left out");
    expect(screen.getByTestId("grill-coverage-count")).toHaveTextContent("1/7");
    expect(screen.getByTestId("grill-area-context")).toHaveAttribute("data-state", "ok");
    expect(screen.getByTestId("grill-area-people")).toHaveAttribute("data-state", "none");
    await user.click(screen.getByTestId("grill-keep-f1"));
    await waitFor(() => {
      expect(kept).toEqual({});
    });
    expect(await screen.findByTestId("grill-kept-f1")).toHaveTextContent("Kept");
  });

  it("fixes a fact, and says when the fix says more than the person's words", async () => {
    const texts: unknown[] = [];
    const taken = view({
      proposals: [FACT],
      turns: [turn(1, "question", "Q"), turn(2, "answer", "It took 7 months.")],
    });
    routes({
      "GET /api/v1/profile/grill/0": { body: taken },
      "POST /api/v1/profile/grill/proposals/f1/keep": async (request) => {
        const sent = (await request.json()) as { text: string };
        texts.push(sent.text);
        return sent.text.includes("9")
          ? {
              status: 400,
              body: { title: "Bad Request", status: 400, code: "fact_not_supported", detail: "x" },
            }
          : {
              body: {
                ...taken,
                proposals: [{ ...FACT, text: sent.text, status: "kept" }],
                kept: 1,
              },
            };
      },
    });
    const user = userEvent.setup();
    renderPage({ position: 0 });
    await user.click(await screen.findByTestId("grill-fix-f1"));
    const field = screen.getByTestId("grill-fix-text-f1");
    expect(field).toHaveValue("Took 7 months");
    await user.clear(field);
    await user.type(field, "Took 9 months");
    await user.click(screen.getByTestId("grill-fix-save-f1"));
    await waitFor(() => {
      expect(field).toHaveAccessibleDescription(/Your words and your profile don’t say that/);
    });
    await user.clear(field);
    await user.type(field, "Took seven months");
    await user.click(screen.getByTestId("grill-fix-save-f1"));
    expect(await screen.findByTestId("grill-kept-f1")).toBeVisible();
    expect(texts).toEqual(["Took 9 months", "Took seven months"]);
    expect(screen.queryByTestId("grill-fix-text-f1")).toBeNull();
  });

  it("drops a fact, and keeps all that wait", async () => {
    const three = view({
      proposals: [
        FACT,
        { ...FACT, id: "f2", text: "2 helped", quote: "2 helped" },
        { ...FACT, id: "f3", text: "Used 3 tools", quote: "3 tools" },
      ],
      turns: [turn(1, "question", "Q"), turn(2, "answer", "A 7 and 2 helped, 3 tools")],
      question: null,
    });
    const rest = three.proposals.slice(1);
    const requests = routes({
      "GET /api/v1/profile/grill/0": { body: three },
      "POST /api/v1/profile/grill/proposals/f1/drop": {
        body: { ...three, proposals: [{ ...FACT, status: "dropped" }, ...rest] },
      },
      "POST /api/v1/profile/grill/0/keep-all": {
        body: {
          ...three,
          proposals: [
            { ...FACT, status: "dropped" },
            ...rest.map((fact) => ({ ...fact, status: "kept" })),
          ],
        },
      },
    });
    const user = userEvent.setup();
    renderPage({ position: 0 });
    await user.click(await screen.findByTestId("grill-drop-f1"));
    expect(await screen.findByTestId("grill-dropped-f1")).toHaveTextContent("Dropped");
    await user.click(await screen.findByTestId("grill-keep-all-2"));
    expect(await screen.findByTestId("grill-kept-f2")).toBeVisible();
    expect(screen.getByTestId("grill-kept-f3")).toBeVisible();
    expect(screen.queryByTestId("grill-keep-all-2")).toBeNull();
    expect(requests.map(({ route }) => route)).toContain("POST /api/v1/profile/grill/0/keep-all");
  });

  it("skips, and leaves for now without composing a question", async () => {
    const requests = routes({
      "POST /api/v1/profile/grill/0/skip": {
        body: view({
          turns: [turn(1, "question", "Q1"), turn(2, "note", "skipped"), turn(3, "question", "Q2")],
          question: { text: "Q2", area: "your_part", push: false },
        }),
      },
      "POST /api/v1/profile/grill/0/done": {
        body: view({
          turns: [turn(1, "question", "Q1"), turn(2, "note", "done")],
          question: null,
        }),
      },
    });
    const user = userEvent.setup();
    renderPage({ position: 0 });
    await user.click(await screen.findByTestId("grill-skip"));
    expect(await screen.findByTestId("grill-turn-3")).toHaveTextContent("Q2");
    expect(screen.getByTestId("grill-note-2")).toHaveTextContent("You skipped this one.");
    await user.click(screen.getByTestId("grill-done"));
    expect(await screen.findByTestId("grill-note-2")).toHaveTextContent("Left for now");
    expect(screen.queryByTestId("grill-answer")).toBeNull();
    expect(screen.getByTestId("grill-continue")).toBeVisible();
    expect(requests.map(({ route }) => route)).toEqual(
      expect.arrayContaining([
        "POST /api/v1/profile/grill/0/skip",
        "POST /api/v1/profile/grill/0/done",
      ]),
    );
  });

  it("says a role is covered, and has nothing more to ask", async () => {
    routes({
      "GET /api/v1/profile/grill/0": {
        body: view({ status: "covered", question: null, turns: [turn(1, "note", "done")] }, 7),
      },
    });
    renderPage({ position: 0 });
    expect(await screen.findByTestId("grill-covered")).toHaveTextContent(
      "All seven areas are covered",
    );
    expect(screen.getByTestId("grill-coverage-count")).toHaveTextContent("7/7");
    expect(screen.queryByTestId("grill-answer")).toBeNull();
  });

  it("says plainly when the AI budget or the daily cap is reached, and keeps the answer", async () => {
    let code = "ai_budget_reached";
    routes({
      "POST /api/v1/profile/grill/0/answer": () => ({
        status: code === "grill_daily_cap" ? 429 : 503,
        body: { title: "x", status: 503, code, detail: "x" },
      }),
    });
    const user = userEvent.setup();
    renderPage({ position: 0 });
    const answer = await screen.findByTestId("grill-answer");
    await user.type(answer, "My answer{Enter}");
    expect(await screen.findByTestId("grill-error")).toHaveTextContent(
      "You’ve reached the AI spending limit, so nothing was sent. Your answer is still in the box.",
    );
    expect(answer).toHaveValue("My answer");
    code = "grill_daily_cap";
    await user.click(screen.getByTestId("grill-send"));
    await waitFor(() => {
      expect(screen.getByTestId("grill-error")).toHaveTextContent("That’s 150 questions today");
    });
  });

  it("says a role that is not there is not there", async () => {
    routes({
      "GET /api/v1/profile/grill/9": {
        status: 404,
        body: { title: "Not Found", status: 404, code: "not_found", detail: "x" },
      },
    });
    renderPage({ position: 9 });
    expect(await screen.findByTestId("grill-role-error")).toHaveTextContent("isn’t there any more");
  });
});
