import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { AboutYouPage } from "./AboutYouPage";

const sentence = (id: string, kind: string, text: string, extra: Record<string, unknown> = {}) => ({
  id,
  text,
  kind,
  why: null,
  downgraded: false,
  topic: "context",
  position: null,
  urls: [],
  new: false,
  choice: null,
  ...extra,
});

const SENTENCES = [
  sentence("s1", "known", "You were Senior Engineer at Example Org."),
  sentence("s2", "told", "You told me: Moved invoicing onto PostgreSQL"),
  sentence("s3", "assumed", "I assume you led a small team.", {
    claim: "Led a small team",
    why: "Senior titles often mean leading people.",
    position: 0,
  }),
  sentence("s4", "assumed", "You run Kubernetes in production.", { downgraded: true }),
];

const EMPLOYERS = [
  { position: 0, company: "Example Org", site: null, state: "no_site", facts: 0, error: null },
];

const NO_QUESTIONS = {
  current: null,
  answered: 0,
  skipped: 0,
  total: 3,
  left: 3,
  done: false,
  strength: 45,
  updated: [],
};

const questionsOf = (overrides: Record<string, unknown> = {}) => ({
  ...NO_QUESTIONS,
  ...overrides,
});

const ASKED = {
  id: "assumed:s3",
  kind: "assumed",
  position: 0,
  area: "people",
  question: "How many people did you lead at Example Org?",
  why: "Your title was Senior Engineer, which often means leading people. Your profile doesn’t say how many.",
  tag: "assumed",
  choices: ["1 to 2", "3 to 6", "More than 6"],
  strength_after: 45,
};

const view = (overrides: Record<string, unknown> = {}) => ({
  has_profile: true,
  profile_version: 3,
  built: true,
  sentences: SENTENCES,
  employers: EMPLOYERS,
  accuracy: null,
  extra: null,
  changes: [],
  questions: NO_QUESTIONS,
  calls_today: 1,
  daily_cap: 60,
  extra_max: 4000,
  ...overrides,
});

const FLAGGED = {
  id: "c1",
  field: "summary",
  value: "Ran events. Now at Acme.",
  position: null,
  quote: "I ran events",
  flags: [{ kind: "claim", claim: "Acme" }],
  resolution: null,
};

const CHANGES = [
  FLAGGED,
  {
    id: "c2",
    field: "skill",
    value: "Elixir",
    position: null,
    quote: "Elixir",
    flags: [],
    resolution: null,
  },
];

function bodyOf(request: Request): Promise<unknown> {
  return request.json();
}

function renderPage(routes: Record<string, Stub> = {}, onBack = vi.fn()) {
  const calls = stubApi({ "GET /api/v1/profile/about": { body: view() }, ...routes });
  renderWithProviders(<AboutYouPage onBack={onBack} spend={<p data-testid="spend">spend</p>} />);
  return calls;
}

describe("Tell me about you", () => {
  it("makes the portrait when there is none for this profile version, saying what it reads", async () => {
    let built = false;
    const calls = renderPage({
      "GET /api/v1/profile/about": () => ({
        body: view({ built, sentences: built ? SENTENCES : [] }),
      }),
      "POST /api/v1/profile/about/portrait": () => {
        built = true;
        return { body: view() };
      },
    });
    expect(await screen.findByTestId("about-making")).toHaveTextContent(
      "Reading your profile and your past employers’ websites…",
    );
    expect(await screen.findByTestId("about-portrait")).toBeVisible();
    expect(calls.filter((c) => c.route === "POST /api/v1/profile/about/portrait")).toHaveLength(1);
  });

  it("shows the portrait by where each sentence comes from, and an assumption opens its reason and three choices by keyboard", async () => {
    const user = userEvent.setup();
    let sent: unknown;
    let choice: string | null = null;
    renderPage({
      "GET /api/v1/profile/about": () => ({
        body: view({
          sentences: SENTENCES.map((s) => (s.id === "s3" ? { ...s, choice } : s)),
        }),
      }),
      "PUT /api/v1/profile/about/sentences/s3": async (request) => {
        sent = await bodyOf(request);
        choice = "right";
        return {
          body: view({ sentences: SENTENCES.map((s) => (s.id === "s3" ? { ...s, choice } : s)) }),
        };
      },
    });
    const portrait = await screen.findByTestId("about-portrait");
    expect(within(portrait).getByTestId("about-sentence-s1")).toHaveAttribute("data-kind", "known");
    expect(within(portrait).getByTestId("about-sentence-s2")).toHaveAttribute("data-kind", "told");
    const assumed = within(portrait).getByTestId("about-sentence-s3");
    expect(assumed).toHaveAttribute("data-kind", "assumed");
    // Reachable without a mouse.
    assumed.focus();
    await user.keyboard("{Enter}");
    const panel = await screen.findByTestId("about-panel");
    expect(panel).toHaveTextContent("I assumed: I assume you led a small team.");
    expect(panel).toHaveTextContent("Why: Senior titles often mean leading people.");
    expect(screen.getByTestId("about-claim")).toHaveTextContent(
      "If right, I’ll save: Led a small team",
    );
    expect(screen.getByTestId("about-choice-right")).toHaveTextContent("That’s right");
    expect(screen.getByTestId("about-choice-wrong")).toHaveTextContent("That’s wrong");
    expect(screen.getByTestId("about-choice-later")).toHaveTextContent("Ask me about it later");
    await user.click(screen.getByTestId("about-choice-right"));
    await waitFor(() => {
      expect(screen.getByTestId("about-sentence-s3")).toHaveAttribute("data-kind", "confirmed");
    });
    expect(sent).toEqual({ choice: "right" });
  });

  it("says a downgraded sentence is not in the profile in these words", async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByTestId("about-sentence-s4"));
    expect(await screen.findByTestId("about-panel")).toHaveTextContent(
      "This isn’t in your profile in these words.",
    );
  });

  it("stores how accurate it is and asks for a site only where none is known", async () => {
    const user = userEvent.setup();
    let accuracy: unknown;
    let site: unknown;
    renderPage({
      "PUT /api/v1/profile/about/accuracy": async (request) => {
        accuracy = await bodyOf(request);
        return { body: view({ accuracy: "some" }) };
      },
      "PUT /api/v1/profile/about/employers/0/site": async (request) => {
        site = await bodyOf(request);
        return {
          body: view({
            employers: [{ ...EMPLOYERS[0], site: "https://example-org.test/", state: "unread" }],
          }),
        };
      },
    });
    await user.click(await screen.findByTestId("about-accuracy-some"));
    expect(accuracy).toEqual({ accuracy: "some" });
    await user.type(screen.getByTestId("about-site-0"), "https://example-org.test");
    await user.click(screen.getByTestId("about-site-add-0"));
    expect(site).toEqual({ url: "https://example-org.test" });
    expect(await screen.findByTestId("about-update")).toBeVisible();
  });

  it("turns what they wrote into changes, flags a claim that isn’t theirs, and lets them use their own words", async () => {
    const user = userEvent.setup();
    let sent: unknown;
    let resolved = false;
    renderPage({
      "POST /api/v1/profile/about/extra": async (request) => {
        sent = await bodyOf(request);
        return {
          body: view({
            extra: "I ran events. Elixir.",
            changes: CHANGES,
            sentences: [...SENTENCES, sentence("tc1", "told", "You ran events.", { new: true })],
          }),
        };
      },
      "PUT /api/v1/profile/about/changes/c1": () => {
        resolved = true;
        return {
          body: view({
            extra: "I ran events. Elixir.",
            changes: [
              { ...CHANGES[0], value: "I ran events. Elixir.", flags: [], resolution: "own_words" },
              CHANGES[1],
            ],
          }),
        };
      },
    });
    await user.type(await screen.findByTestId("about-extra"), "I ran events. Elixir.");
    await user.click(screen.getByTestId("about-continue"));
    expect(sent).toEqual({ text: "I ran events. Elixir." });
    const added = await screen.findByTestId("about-added");
    expect(added).toHaveTextContent("Added from what you wrote");
    expect(screen.getByTestId("about-flags-c1")).toHaveTextContent("“Acme” isn’t in your words.");
    expect(screen.getByTestId("about-added-step")).toHaveTextContent(
      "Questions about the gaps come next.",
    );
    // The new sentence is highlighted as new.
    expect(screen.getByTestId("about-sentence-tc1")).toHaveAttribute("data-fresh");
    await user.click(screen.getByTestId("about-own-words-c1"));
    await waitFor(() => {
      expect(resolved).toBe(true);
      expect(screen.queryByTestId("about-flags-c1")).toBeNull();
    });
  });

  it("reviews every line, holds Save back while a flag waits, and saves in one go", async () => {
    const user = userEvent.setup();
    let flagged = true;
    let saved = false;
    const review = () => ({
      sentences: SENTENCES.slice(0, 2),
      groups: [
        {
          key: "summary",
          position: null,
          title: null,
          rows: [
            {
              id: "c1",
              source: "told",
              text: "Ran events. Now at Acme.",
              field: "summary",
              flags: flagged ? [{ kind: "claim", claim: "Acme" }] : [],
              resolution: flagged ? null : "true",
            },
          ],
        },
        {
          key: "position",
          position: 0,
          title: "Example Org · Senior Engineer",
          rows: [
            {
              id: "s3",
              source: "confirmed",
              text: "I assume you led a small team.",
              field: null,
              flags: [],
              resolution: null,
            },
          ],
        },
      ],
      open_flags: flagged ? 1 : 0,
      left_out: 1,
    });
    const onBack = vi.fn();
    renderPage(
      {
        "GET /api/v1/profile/about": () => ({
          body: view({
            extra: "x",
            changes: [{ ...FLAGGED, flags: flagged ? FLAGGED.flags : [] }],
            questions: questionsOf({ done: true, left: 0, total: 0 }),
          }),
        }),
        "GET /api/v1/profile/about/review": () => ({ body: review() }),
        "PUT /api/v1/profile/about/changes/c1": () => {
          flagged = false;
          return { body: view({ changes: [] }) };
        },
        "POST /api/v1/profile/about/save": () => {
          saved = true;
          return { body: { profile_version: 4, strength: 82, saved: 2, skipped: 0 } };
        },
      },
      onBack,
    );
    // A text was written before: Continue goes on without another call.
    expect(await screen.findByTestId("about-extra")).toHaveValue("x");
    await user.click(screen.getByTestId("about-continue"));
    await user.click(await screen.findByTestId("about-to-questions"));
    // No gap is left to ask about: on to the review.
    const group = await screen.findByTestId("about-group-summary");
    expect(group).toHaveTextContent("You told me");
    expect(screen.getByTestId("about-group-position-0")).toHaveTextContent("Confirmed");
    expect(screen.getByTestId("about-left-out")).toHaveTextContent(
      "1 assumption you didn’t confirm",
    );
    expect(screen.getByTestId("about-save")).toBeDisabled();
    await user.click(screen.getByTestId("about-true-c1"));
    await waitFor(() => {
      expect(screen.getByTestId("about-save")).toBeEnabled();
    });
    await user.click(screen.getByTestId("about-save"));
    expect(await screen.findByTestId("about-saved")).toHaveTextContent("82% strong");
    expect(saved).toBe(true);
    await user.click(screen.getByTestId("about-saved-done"));
    expect(onBack).toHaveBeenCalled();
  });

  it("says plainly when the spending limit is reached", async () => {
    const user = userEvent.setup();
    renderPage({
      "POST /api/v1/profile/about/extra": {
        status: 503,
        body: { code: "ai_budget_reached", title: "x", status: 503, detail: "x" },
      },
    });
    await user.type(await screen.findByTestId("about-extra"), "Hello");
    await user.click(screen.getByTestId("about-continue"));
    expect(await screen.findByTestId("about-error")).toHaveTextContent(
      "You’ve reached the AI spending limit",
    );
  });

  it("asks for a profile first when there is none", async () => {
    renderPage({
      "GET /api/v1/profile/about": {
        body: view({ has_profile: false, built: false, sentences: [] }),
      },
    });
    expect(await screen.findByTestId("about-no-profile")).toHaveTextContent(
      "Add your profile first",
    );
  });

  describe("the questions about the gaps", () => {
    /** The wizard's state, which the stubbed questions change. */
    function questionsState(first: Record<string, unknown> = ASKED) {
      let state = view();
      const set = (overrides: Record<string, unknown>) => {
        state = view(overrides);
        return { body: state };
      };
      return { get: () => ({ body: state }), set, first };
    }

    it("goes from the portrait into the questions: question n of about m, the strength it would add, and why it asks", async () => {
      const user = userEvent.setup();
      const wizard = questionsState();
      const calls = renderPage({
        "GET /api/v1/profile/about": wizard.get,
        "POST /api/v1/profile/about/questions/next": () =>
          wizard.set({ questions: questionsOf({ current: { ...ASKED, strength_after: 53 } }) }),
      });
      await user.click(await screen.findByTestId("about-continue"));
      expect(await screen.findByTestId("about-q")).toBeVisible();
      expect(screen.getByTestId("about-q-progress")).toHaveTextContent("Question 1 of about 3");
      expect(screen.getByTestId("about-q-strength")).toHaveTextContent(
        "Profile strength 45% → 53% if you answer",
      );
      expect(screen.getByTestId("about-q-question")).toHaveTextContent(
        "How many people did you lead at Example Org?",
      );
      expect(screen.getByTestId("about-q-tag")).toHaveTextContent("I assumed");
      expect(screen.getByTestId("about-q-why-text")).toHaveTextContent(
        "Your profile doesn’t say how many.",
      );
      expect(screen.getAllByTestId(/^about-q-choice-/)).toHaveLength(3);
      expect(screen.getByTestId("about-q-cost")).toHaveTextContent("1 small AI call");
      expect(screen.getByTestId("about-q-peek")).toHaveTextContent("See what I know about you now");
      expect(screen.getByTestId("about-q-portrait")).toBeInTheDocument();
      // One call worded it, and only one.
      expect(
        calls.filter((c) => c.route === "POST /api/v1/profile/about/questions/next"),
      ).toHaveLength(1);
    });

    it("answers with a choice and words, shows the update with where it goes and its flag, and asks the next", async () => {
      const user = userEvent.setup();
      const wizard = questionsState();
      let sent: unknown;
      const update = {
        id: "q1",
        field: "role_fact",
        value: "Led about 4 people at Example Org",
        position: 0,
        quote: "about 4 people",
        flags: [{ kind: "claim", claim: "Example Org" }],
        resolution: null,
      };
      const second = {
        ...ASKED,
        id: "results:0",
        kind: "results",
        area: "results",
        question: "What changed because of the billing platform?",
        why: "Your profile states no results for Senior Engineer at Example Org.",
        tag: "unknown",
        choices: [],
      };
      let asked = 0;
      renderPage({
        "GET /api/v1/profile/about": wizard.get,
        "POST /api/v1/profile/about/questions/next": () => {
          asked += 1;
          return wizard.set({
            questions: questionsOf({
              current: asked === 1 ? { ...ASKED, kind: "team_size" } : second,
              answered: asked - 1,
              updated: asked === 1 ? [] : ["q1"],
            }),
            changes: asked === 1 ? [] : [update],
          });
        },
        "POST /api/v1/profile/about/questions/assumed%3As3/answer": async (request) => {
          sent = await bodyOf(request);
          return wizard.set({
            questions: questionsOf({ answered: 1, updated: ["q1"] }),
            changes: [update],
          });
        },
      });
      await user.click(await screen.findByTestId("about-continue"));
      await user.click(await screen.findByTestId("about-q-choice-1"));
      expect(screen.getByTestId("about-q-choice-1")).toHaveAttribute("aria-pressed", "true");
      // A closed question with one answer: another choice replaces it.
      await user.click(screen.getByTestId("about-q-choice-0"));
      expect(screen.getByTestId("about-q-choice-1")).toHaveAttribute("aria-pressed", "false");
      await user.type(screen.getByTestId("about-q-answer"), "about 4 people");
      await user.click(screen.getByTestId("about-q-send"));
      expect(await screen.findByTestId("about-q-updated")).toHaveTextContent(
        "Updated from your answer",
      );
      expect(sent).toEqual({ text: "about 4 people", choices: ["1 to 2"] });
      expect(screen.getByTestId("about-q-updated-q1")).toHaveTextContent(
        "Goes to: Example Org · role.",
      );
      expect(screen.getByTestId("about-flags-q1")).toHaveTextContent(
        "“Example Org” isn’t in your words.",
      );
      // The next question is for another gap, and says it does not know.
      expect(await screen.findByTestId("about-q-question")).toHaveTextContent(
        "What changed because of the billing platform?",
      );
      expect(screen.getByTestId("about-q-tag")).toHaveTextContent("I don’t know");
      expect(screen.getByTestId("about-q-progress")).toHaveTextContent("Question 2 of about 3");
      expect(screen.queryByTestId("about-q-choice-0")).toBeNull();
    });

    it("skips without a call of its own, and ends with “That’s enough for now” on the review", async () => {
      const user = userEvent.setup();
      const wizard = questionsState();
      const calls = renderPage({
        "GET /api/v1/profile/about": wizard.get,
        "POST /api/v1/profile/about/questions/next": () =>
          wizard.set({ questions: questionsOf({ current: ASKED }) }),
        "POST /api/v1/profile/about/questions/assumed%3As3/skip": () =>
          wizard.set({ questions: questionsOf({ skipped: 1, left: 2, total: 3 }) }),
        "POST /api/v1/profile/about/questions/enough": () =>
          wizard.set({ questions: questionsOf({ skipped: 1, done: true, left: 0, total: 1 }) }),
        "GET /api/v1/profile/about/review": {
          body: { sentences: [], groups: [], open_flags: 0, left_out: 0 },
        },
      });
      await user.click(await screen.findByTestId("about-continue"));
      await user.click(await screen.findByTestId("about-q-skip"));
      await waitFor(() => {
        expect(calls.map((c) => c.route)).toContain(
          "POST /api/v1/profile/about/questions/assumed%3As3/skip",
        );
      });
      // The next question is asked for after a skip (one more call to word it).
      await waitFor(() => {
        expect(
          calls.filter((c) => c.route === "POST /api/v1/profile/about/questions/next"),
        ).toHaveLength(2);
      });
      await user.click(await screen.findByTestId("about-q-enough"));
      expect(await screen.findByTestId("about-review-step")).toBeVisible();
    });

    it("says plainly when the daily limit is reached, and offers the review", async () => {
      const user = userEvent.setup();
      renderPage({
        "POST /api/v1/profile/about/questions/next": {
          status: 429,
          body: { code: "about_daily_cap", title: "x", status: 429, detail: "x" },
        },
        "GET /api/v1/profile/about/review": {
          body: { sentences: [], groups: [], open_flags: 0, left_out: 0 },
        },
      });
      await user.click(await screen.findByTestId("about-continue"));
      expect(await screen.findByTestId("about-q-error")).toHaveTextContent(
        "limited to 60 AI steps a day",
      );
      await user.click(screen.getByTestId("about-q-to-review"));
      expect(await screen.findByTestId("about-review-step")).toBeVisible();
    });
  });
});
