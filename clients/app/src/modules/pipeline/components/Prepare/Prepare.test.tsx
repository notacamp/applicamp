import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Schema } from "~/lib/api";

import { cv, cvDraft, opportunity } from "../../../../../test/pipeline";
import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { PreparePage } from "./PreparePage";

type Brief = Schema<"Brief">;

const ID = "0190a1b2-0000-7000-8000-000000000001";

const ESTIMATES = { company_read_eur: 0.002, analysis_eur: 0.002, answer_eur: 0.0005 };
const SPEND = {
  today_eur: 0.01,
  day_limit_eur: 0.3,
  month_eur: 0.42,
  month_limit_eur: 5,
  reads_today: 1,
  reads_cap: 20,
};
const LATEST = { position: 0, title: "Senior Engineer", company: "Example Org" };

function brief(overrides: Partial<Brief> = {}): Brief {
  return {
    opportunity_id: ID,
    company: "Example Co",
    site: "https://example.com/",
    site_source: "apply_host",
    site_needed: false,
    read: null,
    facts: [],
    wants: null,
    questions: [],
    latest_role: LATEST,
    estimates: ESTIMATES,
    spend: SPEND,
    ...overrides,
  };
}

const read = {
  read_at: "2026-10-31T10:00:00Z",
  fresh_until: "2026-11-30T10:00:00Z",
  pages: [
    { url: "https://example.com/jobs/1", fetched_at: "2026-10-31T10:00:00Z", kind: "posting" },
    { url: "https://example.com/about", fetched_at: "2026-10-31T10:00:00Z", kind: "linked" },
  ],
  pages_blocked: 1,
  facts_dropped: 2,
};

const fact = (id: string, text: string, removed = false) => ({
  id,
  text,
  quote: `${text} (quote)`,
  url: "https://www.example.com/blog/hotwire",
  fetched_at: "2026-10-31T10:00:00Z",
  removed,
});

const question = (
  id: string,
  overrides: Partial<Schema<"BriefQuestion">> = {},
): Schema<"BriefQuestion"> => ({
  id,
  question: "They are moving to Hotwire. Have you used it?",
  about: { position: 1, title: "Engineer", company: "Sample Labs" },
  skill: "Hotwire",
  state: "open",
  ...overrides,
});

function renderPage(routes: Record<string, Stub>) {
  const requests = stubApi({
    [`GET /api/v1/opportunities/${ID}`]: { body: opportunity({ id: ID, stage: "shortlisted" }) },
    [`GET /api/v1/opportunities/${ID}/cv`]: { body: cv({ opportunity_id: ID }) },
    ...routes,
  });
  const view = renderWithProviders(
    <PreparePage
      id={ID}
      onBack={vi.fn()}
      onReview={vi.fn()}
      renderFacts={(answer) => <p data-testid="facts-slot">{answer.proposals.length} facts</p>}
      spend={<p data-testid="month-spend">AI this month</p>}
    />,
  );
  return { requests, ...view };
}

const BRIEF = `GET /api/v1/opportunities/${ID}/brief`;

describe("Prepare: company and job", () => {
  it("offers Read with its cost, asks nothing of the model on opening, and leaves a spot for the draft", async () => {
    const { requests } = renderPage({ [BRIEF]: { body: brief() } });
    expect(await screen.findByTestId("prepare-read")).toHaveTextContent(
      "Read about Example Co · about €0.004",
    );
    expect(screen.getByTestId("prepare-site")).toHaveTextContent("example.com");
    expect(screen.getByTestId("prepare-wants-pending")).toBeVisible();
    expect(screen.getByTestId("prepare-draft-slot")).toBeVisible();
    expect(await screen.findByTestId("prepare-draft")).toHaveTextContent(
      "Draft the CV · about €0.002",
    );
    expect(screen.getByTestId("prepare-spend")).toHaveTextContent("Company reads today: 1 of 20.");
    expect(requests.map((r) => r.route)).not.toContain(
      `POST /api/v1/opportunities/${ID}/brief/read`,
    );
  });

  it("asks for the website once when nothing gives it, and keeps it", async () => {
    let saved: unknown;
    renderPage({
      [BRIEF]: { body: brief({ site: null, site_source: null, site_needed: true }) },
      [`PUT /api/v1/opportunities/${ID}/brief/site`]: async (request) => {
        saved = await request.json();
        return { body: brief() };
      },
    });
    const input = await screen.findByTestId("prepare-site-input");
    expect(screen.getByTestId("prepare-read")).toBeDisabled();
    const user = userEvent.setup();
    await user.type(input, "https://example.com/about");
    await user.click(screen.getByTestId("prepare-site-save"));
    await waitFor(() => {
      expect(screen.getByTestId("prepare-site")).toHaveTextContent("example.com");
    });
    expect(saved).toEqual({ url: "https://example.com/about" });
    expect(screen.getByTestId("prepare-read")).toBeEnabled();
  });

  it("reads, shows the facts with their pages and what the job wants, and removes a fact with Undo", async () => {
    let state = brief();
    const post = (route: string, next: () => Brief): Record<string, Stub> => ({
      [route]: () => {
        state = next();
        return { body: state };
      },
    });
    renderPage({
      [BRIEF]: () => ({ body: state }),
      ...post(`POST /api/v1/opportunities/${ID}/brief/read`, () =>
        brief({
          read,
          facts: [fact("a", "Builds ledgers"), fact("b", "Is moving to Hotwire")],
          wants: [
            { label: "Rails", quote: "5+ years of Rails", level: "must" },
            { label: "Kafka", quote: "Kafka is a plus", level: "nice" },
          ],
        }),
      ),
      ...post(`DELETE /api/v1/opportunities/${ID}/brief/facts/b`, () =>
        brief({
          read,
          facts: [fact("a", "Builds ledgers"), fact("b", "Is moving to Hotwire", true)],
          wants: [{ label: "Rails", quote: "5+ years of Rails", level: "must" }],
        }),
      ),
      ...post(`POST /api/v1/opportunities/${ID}/brief/facts/b/restore`, () =>
        brief({
          read,
          facts: [fact("a", "Builds ledgers"), fact("b", "Is moving to Hotwire")],
          wants: [{ label: "Rails", quote: "5+ years of Rails", level: "must" }],
        }),
      ),
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("prepare-read"));

    const facts = await screen.findByTestId("prepare-facts");
    expect(within(facts).getByTestId("prepare-fact-a")).toHaveTextContent("Builds ledgers");
    expect(within(facts).getByTestId("prepare-fact-b")).toHaveTextContent(
      "Is moving to Hotwire (quote)",
    );
    // The source link shows host and path.
    expect(screen.getByTestId("prepare-fact-source-b")).toHaveTextContent(
      "example.com/blog/hotwire",
    );
    expect(screen.getByTestId("prepare-fact-source-b")).toHaveAttribute(
      "href",
      "https://www.example.com/blog/hotwire",
    );
    expect(screen.getByTestId("prepare-pages")).toHaveTextContent("Read 2 pages on");
    expect(screen.getByTestId("prepare-pages")).toHaveTextContent("example.com/about");
    expect(screen.getByTestId("prepare-blocked")).toHaveTextContent("1 page was left alone");
    expect(screen.getByTestId("prepare-dropped")).toHaveTextContent("2 facts were left out");
    expect(screen.getByTestId("prepare-want-0")).toHaveTextContent("Rails");
    expect(screen.getByTestId("prepare-want-1")).toHaveTextContent("Kafka · nice to have");
    expect(screen.getByTestId("prepare-read-again")).toHaveTextContent("about €0.002");

    await user.click(screen.getByTestId("prepare-fact-remove-b"));
    const undo = await screen.findByTestId("prepare-fact-undo-b");
    expect(screen.getByTestId("prepare-fact-b")).toHaveTextContent("Removed");
    await user.click(undo);
    expect(await screen.findByTestId("prepare-fact-remove-b")).toBeVisible();
  });

  it("answers a question into facts to keep, and puts a skill away without a model call", async () => {
    const { requests } = renderPage({
      [BRIEF]: {
        body: brief({
          read,
          facts: [fact("a", "Builds ledgers")],
          wants: [{ label: "Hotwire", quote: "Hotwire", level: "must" }],
          questions: [
            question("q1"),
            question("q2", { question: "What did you own?", skill: null, about: null }),
          ],
        }),
      },
      [`POST /api/v1/opportunities/${ID}/brief/questions/q2/answer`]: {
        body: {
          question: question("q2", { state: "answered", skill: null, about: null }),
          role: { position: 0, title: "Senior Engineer", company: "Example Org" },
          defaulted: true,
          proposals: [
            {
              id: "p1",
              text: "Owned invoicing",
              quote: "I owned invoicing",
              area: "your_part",
              expires_at: "2026-11-01T10:00:00Z",
            },
          ],
          dropped: 1,
        },
      },
      [`POST /api/v1/opportunities/${ID}/brief/questions/q1/dont-have`]: {
        body: brief({
          read,
          facts: [fact("a", "Builds ledgers")],
          wants: [{ label: "Hotwire", quote: "Hotwire", level: "must" }],
          questions: [
            question("q1", { state: "dont_have" }),
            question("q2", { question: "What did you own?", skill: null, about: null }),
          ],
        }),
      },
    });
    const user = userEvent.setup();
    await screen.findByTestId("prepare-question-q1");
    // A skill question can be put away; the other cannot.
    expect(screen.getByTestId("prepare-donthave-q1")).toBeVisible();
    expect(screen.queryByTestId("prepare-donthave-q2")).toBeNull();
    expect(screen.getByTestId("prepare-question-goes-q1")).toHaveTextContent(
      "Your answer goes on Engineer · Sample Labs.",
    );
    // No position named: the most recent one, and it says so.
    expect(screen.getByTestId("prepare-question-goes-q2")).toHaveTextContent(
      "your most recent one: Senior Engineer · Example Org",
    );

    await user.type(screen.getByTestId("prepare-answer-q2"), "I owned invoicing.");
    await user.click(screen.getByTestId("prepare-answer-send-q2"));
    const answered = await screen.findByTestId("prepare-answered-q2");
    expect(within(answered).getByTestId("facts-slot")).toHaveTextContent("1 facts");
    expect(answered).toHaveTextContent("your most recent position: Senior Engineer · Example Org");
    expect(answered).toHaveTextContent("1 line was left out");

    await user.click(screen.getByTestId("prepare-donthave-q1"));
    await waitFor(() => {
      expect(screen.queryByTestId("prepare-question-q1")).toBeNull();
    });
    expect(requests.some((r) => r.route.endsWith("/q1/answer"))).toBe(false);
  });

  it("says in plain words why a read failed", async () => {
    renderPage({
      [BRIEF]: { body: brief() },
      [`POST /api/v1/opportunities/${ID}/brief/read`]: {
        status: 409,
        body: {
          type: "about:blank",
          title: "Conflict",
          status: 409,
          detail: "robots",
          code: "robots_disallows",
        },
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("prepare-read"));
    expect(await screen.findByTestId("prepare-error")).toHaveTextContent(
      "The company’s website asks robots not to read it, so nothing was read.",
    );
  });

  it("drafts the CV from a click and goes to the review; with a draft it says Review the CV", async () => {
    const { requests } = renderPage({
      [BRIEF]: { body: brief() },
      [`POST /api/v1/opportunities/${ID}/cv/drafts`]: {
        body: cv({ opportunity_id: ID, draft: cvDraft() }),
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("prepare-draft"));
    await waitFor(() => {
      expect(screen.getByTestId("prepare-review")).toHaveTextContent("Review the CV");
    });
    expect(screen.queryByTestId("prepare-draft")).toBeNull();
    expect(
      requests.filter((r) => r.route === `POST /api/v1/opportunities/${ID}/cv/drafts`),
    ).toHaveLength(1);
  });

  it("turns Draft the CV off, with its reason, when the job is not shortlisted or a cap is hit", async () => {
    renderPage({
      [BRIEF]: { body: brief() },
      [`GET /api/v1/opportunities/${ID}/cv`]: {
        body: cv({
          opportunity_id: ID,
          can_draft: { allowed: false, reason: "daily_cap" },
        }),
      },
    });
    expect(await screen.findByTestId("prepare-draft")).toBeDisabled();
    expect(screen.getByTestId("prepare-draft-reason")).toHaveTextContent(
      "You’ve made 20 drafts today. Drafting continues tomorrow.",
    );
  });
});
