import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { LinkedInPage } from "./LinkedInPage";

const source = (id: string, kind: string, text: string) => ({ id, kind, text });

const proposal = (
  id: string,
  field: string,
  text: string,
  extra: Record<string, unknown> = {},
) => ({
  id,
  field,
  position: null,
  text,
  why: "Says the role you want next.",
  sources: [source("w0", "profile", "Senior Engineer at Example Org. Runs the billing platform.")],
  flags: [],
  status: "ready",
  edited: false,
  ...extra,
});

const PROPOSALS = [
  proposal("p1", "headline", "Senior Engineer · billing platform", {
    sources: [
      source("w0", "profile", "Senior Engineer at Example Org. Runs the billing platform."),
      source("l.headline", "linkedin", "Backend engineer"),
    ],
  }),
  proposal("p2", "summary", "Passionate engineer who runs the billing platform.", {
    flags: [{ kind: "buzzword", claim: "passionate" }],
  }),
  proposal("p3", "position_description", "Runs the billing platform on Kubernetes.", {
    position: 0,
    flags: [{ kind: "claim", claim: "Kubernetes" }],
  }),
  proposal("p4", "skill", "Kafka", {
    sources: [source("w0f0", "told", "Moved the queue to Kafka")],
  }),
];

const FINDINGS = [
  {
    id: "f1",
    part: "headline",
    position: null,
    title: null,
    now: "Backend engineer",
    state: "could_be_better",
    why: "It does not name the role you want.",
    proposals: ["p1"],
  },
  {
    id: "f2",
    part: "summary",
    position: null,
    title: null,
    now: null,
    state: "missing",
    why: "There is no About text.",
    proposals: ["p2"],
  },
  {
    id: "f3",
    part: "position",
    position: 0,
    title: "Senior Engineer at Example Org",
    now: "Runs billing.",
    state: "could_be_better",
    why: "The description says little.",
    proposals: ["p3"],
  },
  {
    id: "f4",
    part: "skills",
    position: null,
    title: null,
    now: "Rust, PostgreSQL",
    state: "could_be_better",
    why: "Kafka is missing.",
    proposals: ["p4"],
  },
  {
    id: "f5",
    part: "education",
    position: null,
    title: null,
    now: "Master's degree, Sample University",
    state: "good",
    why: "It says what it needs to.",
    proposals: [],
  },
];

const review = (proposals: unknown[] = PROPOSALS, extra: Record<string, unknown> = {}) => ({
  kind: "linkedin_zip",
  language: "en",
  made_at: "2026-11-04T10:00:00Z",
  stale: false,
  overall: "Your headline says what you do, not the Staff role you want.",
  findings: FINDINGS,
  proposals,
  open: 4,
  kept: 0,
  done: 0,
  ...extra,
});

const view = (overrides: Record<string, unknown> = {}) => ({
  has_profile: true,
  ai_available: true,
  review: null,
  checkups_today: 0,
  daily_cap: 10,
  ...overrides,
});

function bodyOf(request: Request): Promise<unknown> {
  return request.json();
}

function renderPage(routes: Record<string, Stub> = {}, onBack = vi.fn()) {
  const calls = stubApi({ "GET /api/v1/profile/linkedin": { body: view() }, ...routes });
  renderWithProviders(<LinkedInPage onBack={onBack} />);
  return calls;
}

describe("Your LinkedIn", () => {
  it("asks for the file when there is no review, uploads it and shows the review", async () => {
    const user = userEvent.setup();
    const calls = renderPage({
      "POST /api/v1/profile/linkedin": {
        status: 201,
        body: view({ review: review(), checkups_today: 1 }),
      },
    });
    const upload = await screen.findByTestId("linkedin-upload");
    expect(upload).toHaveTextContent("The file isn’t kept");
    expect(screen.getByTestId("linkedin-kinds")).toHaveTextContent("Your LinkedIn data export");
    expect(screen.getByTestId("linkedin-kinds")).toHaveTextContent("Your profile saved as a PDF");
    // Nothing chosen: said, no request.
    await user.click(screen.getByTestId("linkedin-submit"));
    expect(await screen.findByTestId("linkedin-error")).toHaveTextContent("Choose a file first.");

    const file = new File(["zip"], "Basic_LinkedInDataExport.zip", { type: "application/zip" });
    await user.upload(screen.getByTestId("linkedin-file"), file);
    await user.click(screen.getByTestId("linkedin-submit"));
    expect(await screen.findByTestId("linkedin-review")).toBeVisible();
    expect(calls.filter((c) => c.route === "POST /api/v1/profile/linkedin")).toHaveLength(1);
    expect(screen.getByTestId("linkedin-overall")).toHaveTextContent("not the Staff role you want");
    expect(screen.getByTestId("linkedin-overall")).toHaveTextContent(
      "From your LinkedIn data export.",
    );
  });

  it("shows one block per part with its state, what LinkedIn says now, and the proposals with where they came from", async () => {
    renderPage({ "GET /api/v1/profile/linkedin": { body: view({ review: review() }) } });
    const headline = await screen.findByTestId("linkedin-finding-f1");
    expect(headline).toHaveTextContent("Headline");
    expect(within(headline).getByTestId("linkedin-state-f1")).toHaveAttribute(
      "data-state",
      "could_be_better",
    );
    expect(headline).toHaveTextContent("On LinkedIn now: Backend engineer");
    expect(screen.getByTestId("linkedin-finding-f2")).toHaveTextContent("Nothing there yet.");
    expect(screen.getByTestId("linkedin-state-f2")).toHaveTextContent("Missing");
    expect(screen.getByTestId("linkedin-finding-f3")).toHaveTextContent(
      "Senior Engineer at Example Org",
    );
    expect(screen.getByTestId("linkedin-state-f5")).toHaveTextContent("Good");
    const first = screen.getByTestId("linkedin-proposal-p1");
    expect(first).toHaveAttribute("data-status", "ready");
    expect(first).toHaveTextContent("New headline");
    expect(within(first).getByTestId("linkedin-proposal-text-p1")).toHaveTextContent(
      "Senior Engineer · billing platform",
    );
    expect(first).toHaveTextContent("Where this came from");
    expect(first).toHaveTextContent("Your LinkedIn: Backend engineer");
    expect(screen.getByTestId("linkedin-proposal-p4")).toHaveTextContent(
      "You told me: Moved the queue to Kafka",
    );
    expect(screen.getByTestId("linkedin-todo")).toHaveTextContent("Keep a proposal above");
  });

  it("keeps a proposal, which goes on the list with its text to copy, and marks it done", async () => {
    const user = userEvent.setup();
    const sent: unknown[] = [];
    let status = "ready";
    const current = () =>
      view({
        review: review(
          PROPOSALS.map((p) => (p.id === "p1" ? { ...p, status } : p)),
          { kept: status === "kept" ? 1 : 0, done: status === "done" ? 1 : 0 },
        ),
      });
    renderPage({
      "GET /api/v1/profile/linkedin": () => ({ body: current() }),
      "PATCH /api/v1/profile/linkedin/proposals/p1": async (request) => {
        const body = (await bodyOf(request)) as { status: string };
        sent.push(body);
        status = body.status;
        return { body: current() };
      },
    });
    await user.click(await screen.findByTestId("linkedin-keep-p1"));
    await waitFor(() => {
      expect(screen.getByTestId("linkedin-proposal-p1")).toHaveAttribute("data-status", "kept");
    });
    const todo = screen.getByTestId("linkedin-todo-p1");
    expect(todo).toHaveAttribute("data-status", "kept");
    expect(within(todo).getByTestId("linkedin-copy-p1")).toHaveTextContent("Copy");
    await user.click(within(todo).getByTestId("linkedin-done-p1"));
    await waitFor(() => {
      expect(screen.getByTestId("linkedin-todo-p1")).toHaveAttribute("data-status", "done");
    });
    expect(screen.getByTestId("linkedin-todo-p1")).toHaveTextContent("Done on LinkedIn");
    await user.click(screen.getByTestId("linkedin-undone-p1"));
    await waitFor(() => {
      expect(screen.getByTestId("linkedin-todo-p1")).toHaveAttribute("data-status", "kept");
    });
    expect(sent).toEqual([{ status: "kept" }, { status: "done" }, { status: "kept" }]);
  });

  it("keeps Keep off on a flagged proposal, says why, and an edit is checked again", async () => {
    const user = userEvent.setup();
    let text = "Runs the billing platform on Kubernetes.";
    let flags: unknown[] = [{ kind: "claim", claim: "Kubernetes" }];
    const current = () =>
      view({
        review: review(
          PROPOSALS.map((p) =>
            p.id === "p3" ? { ...p, text, flags, edited: flags.length === 0 } : p,
          ),
        ),
      });
    const edits: unknown[] = [];
    renderPage({
      "GET /api/v1/profile/linkedin": () => ({ body: current() }),
      "PATCH /api/v1/profile/linkedin/proposals/p3": async (request) => {
        const body = (await bodyOf(request)) as { text: string };
        edits.push(body);
        text = body.text;
        flags = body.text.includes("Kubernetes") ? flags : [];
        return { body: current() };
      },
    });
    const row = await screen.findByTestId("linkedin-proposal-p3");
    expect(within(row).getByTestId("linkedin-flags-p3")).toHaveTextContent(
      "Your profile doesn’t say “Kubernetes”.",
    );
    expect(within(row).getByTestId("linkedin-keep-p3")).toBeDisabled();
    expect(row).toHaveTextContent("Keep is off while this says something your profile doesn’t.");
    expect(screen.getByTestId("linkedin-flags-p2")).toHaveTextContent(
      "“passionate” is a word nobody wrote about you.",
    );

    await user.click(within(row).getByTestId("linkedin-edit-p3"));
    const field = within(row).getByTestId("linkedin-edit-text-p3");
    await user.clear(field);
    await user.type(field, "Runs the billing platform.");
    await user.click(within(row).getByTestId("linkedin-save-p3"));
    await waitFor(() => {
      expect(screen.queryByTestId("linkedin-flags-p3")).toBeNull();
    });
    expect(edits).toEqual([{ text: "Runs the billing platform." }]);
    expect(screen.getByTestId("linkedin-keep-p3")).toBeEnabled();
  });

  it("says the profile changed since, offers to check again, and start over throws the review away", async () => {
    const user = userEvent.setup();
    let gone = false;
    renderPage({
      "GET /api/v1/profile/linkedin": () => ({
        body: view({ review: gone ? null : review(PROPOSALS, { stale: true }) }),
      }),
      "DELETE /api/v1/profile/linkedin": () => {
        gone = true;
        return { status: 204 };
      },
    });
    expect(await screen.findByTestId("linkedin-stale")).toHaveTextContent(
      "Your profile changed since this review.",
    );
    await user.click(screen.getByTestId("linkedin-stale-again"));
    expect(await screen.findByTestId("linkedin-upload")).toBeVisible();
    await user.click(screen.getByTestId("linkedin-cancel"));
    expect(await screen.findByTestId("linkedin-review")).toBeVisible();
    await user.click(screen.getByTestId("linkedin-start-over"));
    expect(await screen.findByTestId("linkedin-upload")).toBeVisible();
    expect(screen.queryByTestId("linkedin-cancel")).toBeNull();
  });

  it("says to add the profile first, or that a model is needed, and goes back", async () => {
    const user = userEvent.setup();
    const onBack = vi.fn();
    renderPage({ "GET /api/v1/profile/linkedin": { body: view({ has_profile: false }) } }, onBack);
    expect(await screen.findByTestId("linkedin-no-profile")).toHaveTextContent(
      "Add your profile first",
    );
    await user.click(screen.getByTestId("linkedin-no-profile-back"));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("says the check-up needs an AI model when none is set up", async () => {
    renderPage({ "GET /api/v1/profile/linkedin": { body: view({ ai_available: false }) } });
    expect(await screen.findByTestId("linkedin-no-ai")).toHaveTextContent("This needs an AI model");
    expect(screen.queryByTestId("linkedin-upload")).toBeNull();
  });

  it("stops at the day's cap", async () => {
    renderPage({
      "GET /api/v1/profile/linkedin": { body: view({ checkups_today: 10 }) },
    });
    expect(await screen.findByTestId("linkedin-capped")).toHaveTextContent(
      "You’ve made 10 check-ups today",
    );
    expect(screen.getByTestId("linkedin-submit")).toBeDisabled();
  });
});
