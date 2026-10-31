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
    why: "Senior titles often mean leading people.",
    position: 0,
  }),
  sentence("s4", "assumed", "You run Kubernetes in production.", { downgraded: true }),
];

const EMPLOYERS = [
  { position: 0, company: "Example Org", site: null, state: "no_site", facts: 0, error: null },
];

const view = (overrides: Record<string, unknown> = {}) => ({
  has_profile: true,
  profile_version: 3,
  built: true,
  sentences: SENTENCES,
  employers: EMPLOYERS,
  accuracy: null,
  extra: null,
  changes: [],
  calls_today: 1,
  daily_cap: 30,
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
    await user.click(await screen.findByTestId("about-to-review"));
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
});
