import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { ImproveProfile } from "./ImproveProfile";

const WORK = {
  id: "q1",
  section: "work",
  context: {
    position: "Senior Engineer",
    company: "Example Org",
    start_date: "2021-03",
    end_date: null,
  },
  question: "What did you ship at Example Org that you are proud of?",
  why: "Your latest role has no results yet.",
  expected_gain: 8,
};
const LABEL = {
  id: "q2",
  section: "basics.label",
  context: null,
  question: "In one line, what do you want to be known for?",
  why: "There is no headline yet.",
  expected_gain: 6,
};

const questions = (open = [WORK, LABEL], overrides: Record<string, unknown> = {}) => ({
  status: "ready",
  reason: null,
  score: 40,
  total: 2,
  answered: 2 - open.length,
  questions: open,
  ...overrides,
});

const DRAFT = {
  id: "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5d",
  question_id: "q1",
  kind: "highlights",
  items: ["Rebuilt billing in Rust", "Cut invoice runs from 6 hours to 40 minutes"],
  education: null,
  dropped: 1,
  expires_at: "2026-10-29T10:00:00Z",
};

const PROFILE_VIEW = {
  profile: {
    basics: { profiles: [] },
    work: [],
    education: [],
    skills: [],
    languages: [],
    certificates: [],
    projects: [],
  },
  source: "answer",
  updated_at: "2026-10-28T10:00:00Z",
  import_kinds: ["linkedin_zip"],
};

function routes(extra: Record<string, Stub> = {}) {
  return stubApi({ "GET /api/v1/profile/questions": { body: questions() }, ...extra });
}

describe("Improve my profile", () => {
  it("asks the most useful question with its context and why, and shows what answering is worth", async () => {
    routes();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    const page = await screen.findByTestId("improve-question");
    expect(page).toHaveTextContent("Question 1 of 2");
    expect(page).toHaveTextContent(WORK.question);
    expect(page).toHaveTextContent(WORK.why);
    expect(screen.getByTestId("improve-context")).toHaveTextContent(
      "From your role · Senior Engineer · Example Org · 2021–now",
    );
    expect(screen.getByTestId("improve-progress")).toHaveTextContent("Strength 40% → 48%");
  });

  it("asks for the question it was asked for", async () => {
    routes();
    renderWithProviders(<ImproveProfile question="q2" onLeave={vi.fn()} />);
    expect(await screen.findByTestId("improve-question")).toHaveTextContent(LABEL.question);
    expect(screen.getByTestId("improve-context")).toHaveTextContent("Your headline");
  });

  it("waits while the AI reads the profile, then asks", async () => {
    let asked = 0;
    routes({
      "GET /api/v1/profile/questions": () => {
        asked += 1;
        return asked === 1
          ? { status: 202, body: questions([], { status: "running", total: 0 }) }
          : { body: questions() };
      },
    });
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    expect(await screen.findByTestId("improve-reading")).toHaveTextContent("Reading your profile…");
    expect(await screen.findByTestId("improve-question", {}, { timeout: 8000 })).toBeVisible();
  });

  it("drafts on Enter, a new line on Shift+Enter, and saves nothing until Keep", async () => {
    const requests = routes({
      "POST /api/v1/profile/questions/q1/draft": async (request) => {
        const { answer } = (await request.json()) as { answer: string };
        return { status: 201, body: { ...DRAFT, items: [answer, ...DRAFT.items] } };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    const answer = await screen.findByTestId("improve-answer");
    expect(screen.getByTestId("improve-draft")).toBeDisabled();

    await user.type(answer, "Rebuilt billing{Shift>}{Enter}{/Shift}in Rust");
    expect(answer).toHaveValue("Rebuilt billing\nin Rust");
    await user.keyboard("{Enter}");
    const block = await screen.findByTestId("improve-draft-block");
    expect(block).toHaveTextContent("Rebuilt billing in Rust");
    expect(screen.getByTestId("improve-draft-items")).toHaveTextContent("6 hours to 40 minutes");
    expect(screen.getByTestId("improve-dropped")).toHaveTextContent(
      "1 line left out: it mentioned something you didn’t write.",
    );
    expect(requests.map(({ route }) => route)).not.toContain(
      `POST /api/v1/profile/drafts/${DRAFT.id}/keep`,
    );
  });

  it("keeps the draft, edited, and says so", async () => {
    let kept: unknown;
    routes({
      "POST /api/v1/profile/questions/q1/draft": { status: 201, body: DRAFT },
      [`POST /api/v1/profile/drafts/${DRAFT.id}/keep`]: async (request) => {
        kept = await request.json();
        return { body: PROFILE_VIEW };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    await user.type(await screen.findByTestId("improve-answer"), "Rebuilt billing in Rust{Enter}");
    await screen.findByTestId("improve-draft-block");
    await user.click(screen.getByTestId("improve-edit"));
    const edit = screen.getByTestId("improve-draft-edit");
    await user.clear(edit);
    await user.type(edit, "Rebuilt billing in Rust, 6 hours to 40 minutes");
    await user.click(screen.getByTestId("improve-keep"));
    await waitFor(() => {
      expect(kept).toEqual({ items: ["Rebuilt billing in Rust, 6 hours to 40 minutes"] });
    });
    expect(await screen.findByTestId("improve-kept")).toHaveTextContent("Kept.");
  });

  it("discards a draft without keeping it", async () => {
    const requests = routes({
      "POST /api/v1/profile/questions/q1/draft": { status: 201, body: DRAFT },
      [`POST /api/v1/profile/drafts/${DRAFT.id}/discard`]: { status: 204 },
    });
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    await user.type(await screen.findByTestId("improve-answer"), "Rebuilt billing{Enter}");
    await user.click(await screen.findByTestId("improve-discard"));
    expect(screen.queryByTestId("improve-draft-block")).toBeNull();
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain(
        `POST /api/v1/profile/drafts/${DRAFT.id}/discard`,
      );
    });
  });

  it("says when nothing in the answer could be drafted without adding something", async () => {
    routes({
      "POST /api/v1/profile/questions/q1/draft": {
        status: 201,
        body: { ...DRAFT, items: [], dropped: 2 },
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    await user.type(await screen.findByTestId("improve-answer"), "Stuff{Enter}");
    expect(await screen.findByTestId("improve-draft-empty")).toHaveTextContent(
      "Nothing in your answer could be drafted",
    );
    expect(screen.getByTestId("improve-keep")).toBeDisabled();
    expect(screen.getByTestId("improve-dropped")).toHaveTextContent("2 lines left out");
  });

  it("skips a question", async () => {
    const requests = routes({ "POST /api/v1/profile/questions/q1/skip": { status: 204 } });
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    await user.click(await screen.findByTestId("improve-skip"));
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain(
        "POST /api/v1/profile/questions/q1/skip",
      );
    });
  });

  it("finishes later with the button or with Esc, never forced", async () => {
    routes();
    const onLeave = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={onLeave} />);
    await screen.findByTestId("improve-question");
    await user.click(screen.getByTestId("improve-later"));
    await user.keyboard("{Escape}");
    expect(onLeave).toHaveBeenCalledTimes(2);
    expect(onLeave).toHaveBeenCalledWith("later");
  });

  it("tells a draft that failed over the spending limit, plainly", async () => {
    routes({
      "POST /api/v1/profile/questions/q1/draft": {
        status: 503,
        body: { code: "ai_budget_reached", status: 503, title: "Unavailable" },
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    await user.type(await screen.findByTestId("improve-answer"), "Rebuilt billing{Enter}");
    expect(await screen.findByTestId("improve-draft-error")).toHaveTextContent(
      "reached the AI spending limit",
    );
  });

  it("explains a review that could not be made and tries again when asked", async () => {
    const requests = routes({
      "GET /api/v1/profile/questions": {
        body: questions([], { status: "failed", reason: "ai_budget_reached", total: 0 }),
      },
      "POST /api/v1/profile/questions/review": {
        status: 202,
        body: questions([], { status: "running", total: 0 }),
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    expect(await screen.findByTestId("improve-failed")).toHaveTextContent("AI spending limit");
    await user.click(screen.getByTestId("improve-review-again"));
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain("POST /api/v1/profile/questions/review");
    });
  });

  it("says so when there is no AI model, and leaves the way back", async () => {
    routes({
      "GET /api/v1/profile/questions": {
        body: questions([], { status: "unavailable", reason: "ai_unavailable", total: 0 }),
      },
    });
    const onLeave = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(<ImproveProfile question={undefined} onLeave={onLeave} />);
    expect(await screen.findByTestId("improve-unavailable")).toHaveTextContent("need an AI model");
    await user.click(screen.getByTestId("improve-back"));
    expect(onLeave).toHaveBeenCalledWith("done");
  });

  it("is done when every question is answered or skipped", async () => {
    routes({ "GET /api/v1/profile/questions": { body: questions([], { score: 62 }) } });
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    expect(await screen.findByTestId("improve-done")).toHaveTextContent("62% strong");
  });

  it("offers to try again when the questions cannot be loaded", async () => {
    stubApi({ "GET /api/v1/profile/questions": { status: 500, body: { code: "internal_error" } } });
    renderWithProviders(<ImproveProfile question={undefined} onLeave={vi.fn()} />);
    expect(await screen.findByTestId("improve-load-error")).toBeVisible();
    expect(screen.getByTestId("improve-retry")).toBeVisible();
  });
});
