import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { companies, FABRIKAM, suggestion, watching } from "../../test/companies";
import { estimate, opportunity, search, todayAnswer } from "../../test/pipeline";
import { DEMO_ME, renderApp, stubApi, type Stub } from "../../test/render";

/** Signed in, with Today's answer and the rest of the app's reads. */
function signedIn(today: Stub, extra: Record<string, Stub> = {}) {
  return stubApi({
    "GET /api/v1/me": { body: DEMO_ME },
    "GET /api/v1/today": today,
    "GET /api/v1/opportunities": { body: { opportunities: [] } },
    "GET /api/v1/searches/latest": { body: { search: null } },
    "GET /api/v1/searches/estimate": { body: estimate() },
    "GET /api/v1/runs": { body: { runs: [], available: true, startable: ["runs.probe"] } },
    ...extra,
  });
}

describe("Today", () => {
  it("asks for the goals once the profile is set up", async () => {
    signedIn({ body: todayAnswer("answer_goals") });
    const user = userEvent.setup();
    const { router } = renderApp("/");
    expect(await screen.findByTestId("today-goals")).toHaveTextContent("0 of 11 answered");
    await user.click(screen.getByTestId("today-answer-goals"));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/goals/interview");
    });
  });

  it("offers Search now, not the profile, once the profile and goals are done", async () => {
    const requests = signedIn(
      {
        body: todayAnswer("search", {
          profile: { done: true, no_skills: false, strength_hint: "add_skills" },
        }),
      },
      { "POST /api/v1/searches": { status: 202, body: search({ status: "queued" }) } },
    );
    const user = userEvent.setup();
    renderApp("/");
    expect(await screen.findByTestId("today-search")).toHaveTextContent("Ready for a first search");
    expect(screen.queryByTestId("set-up-profile")).toBeNull();
    expect(screen.getByTestId("today-strength-hint")).toHaveTextContent("Add your skills");
    await user.click(screen.getByTestId("today-search-now"));
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain("POST /api/v1/searches");
    });
  });

  it("says the profile lists no skills yet on Today, instead of the generic hint", async () => {
    signedIn({
      body: todayAnswer("search", {
        profile: { done: true, no_skills: true, strength_hint: "add_skills" },
      }),
    });
    renderApp("/");
    const line = await screen.findByTestId("today-no-skills");
    expect(line).toHaveTextContent(
      "Your profile lists no skills yet. Add them for better matches.",
    );
    expect(screen.getByTestId("no-skills-link")).toHaveAttribute("href", "/profile#skills");
    expect(screen.queryByTestId("today-strength-hint")).toBeNull();
  });

  it("says it on the Pipeline's search line too, linking to Skills", async () => {
    signedIn({
      body: todayAnswer("search", {
        profile: { done: true, no_skills: true, strength_hint: "add_skills" },
      }),
    });
    const user = userEvent.setup();
    const { router } = renderApp("/pipeline");
    expect(await screen.findByTestId("pipeline-no-skills")).toHaveTextContent(
      "Your profile lists no skills yet",
    );
    await user.click(screen.getByTestId("no-skills-link"));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/profile");
    });
    expect(router.state.location.hash).toBe("skills");
  });

  it("says nothing about skills when the profile lists some", async () => {
    signedIn({ body: todayAnswer("search") });
    renderApp("/");
    await screen.findByTestId("today-search");
    expect(screen.queryByTestId("today-no-skills")).toBeNull();
  });

  it("points to the new matches, with the latest search's line", async () => {
    signedIn({
      body: todayAnswer("review_matches", {
        latest_search: search({
          trigger: "nightly",
          finished_at: new Date(Date.now() - 3_600_000).toISOString(),
        }),
      }),
    });
    const user = userEvent.setup();
    const { router } = renderApp("/");
    expect(await screen.findByTestId("today-matches")).toHaveTextContent("New matches: 9");
    expect(screen.getByTestId("today-latest-search")).toHaveTextContent(
      "Last night: 412 jobs read, 9 matched",
    );
    expect(screen.getByTestId("today-review-matches")).toHaveAttribute(
      "href",
      "/pipeline?new=true",
    );
    await user.click(screen.getByTestId("today-review-matches"));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/pipeline");
    });
    expect(router.state.location.search).toEqual({ new: true });
    expect(await screen.findByTestId("pipeline-new-only")).toBeInTheDocument();
  });

  it("says nothing needs you only when that is true", async () => {
    signedIn({ body: todayAnswer("none", { latest_search: search() }) });
    renderApp("/");
    expect(await screen.findByTestId("today-nothing")).toHaveTextContent("Nothing needs you yet");
    expect(screen.getByTestId("today-search-again")).toBeInTheDocument();
  });

  it("says when Today cannot be read, with a way to try again", async () => {
    let up = false;
    signedIn(() =>
      up ? { body: todayAnswer("search") } : { status: 500, body: { code: "internal_error" } },
    );
    const user = userEvent.setup();
    renderApp("/");
    const error = await screen.findByTestId("today-load-error");
    up = true;
    await user.click(error.querySelector("button") ?? error);
    expect(await screen.findByTestId("today-search")).toBeInTheDocument();
  });

  it("opens New and starts a search from the keyboard, from any page", async () => {
    const requests = signedIn(
      { body: todayAnswer("none") },
      { "POST /api/v1/searches": { status: 202, body: search({ status: "queued" }) } },
    );
    const user = userEvent.setup();
    const { router } = renderApp("/agents");
    await screen.findByTestId("agents-empty");

    await user.keyboard("n");
    expect(await screen.findByTestId("new-opportunity-url")).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/pipeline");
    await user.click(screen.getByTestId("new-opportunity-cancel"));
    await waitFor(() => {
      expect(router.state.location.search).toEqual({});
    });

    await user.keyboard("{Shift>}S{/Shift}");
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain("POST /api/v1/searches");
    });
    await waitFor(() => {
      expect(router.state.location.search).toEqual({});
    });
  });

  it("tells that a watched company posted a job that fits, and opens it in the Pipeline", async () => {
    const hit = "0190a1b2-0000-7000-8000-0000000000e1";
    signedIn(
      {
        body: todayAnswer("review_matches", {
          company_hits: [
            {
              id: hit,
              company: "Northwind Stays",
              title: "Senior Rails Engineer",
              location: "Remote - Europe",
              work_mode: "remote",
              salary_text: "EUR 85000–100000 per year",
              match_score: 86,
              found_at: new Date().toISOString(),
            },
          ],
        }),
      },
      {
        "GET /api/v1/companies": { body: companies([], []) },
        [`GET /api/v1/opportunities/${hit}`]: {
          body: opportunity({
            id: hit,
            company: "Northwind Stays",
            title: "Senior Rails Engineer",
          }),
        },
        [`PATCH /api/v1/opportunities/${hit}`]: {
          body: opportunity({ id: hit, company: "Northwind Stays", new: false }),
        },
      },
    );
    const user = userEvent.setup();
    const { router } = renderApp("/");
    const card = await screen.findByTestId(`today-hit-${hit}`);
    expect(card).toHaveTextContent("Northwind Stays posted a job that fits you");
    expect(card).toHaveTextContent(
      "Senior Rails Engineer · Remote - Europe · EUR 85000–100000 per year",
    );
    const found = screen.getByTestId(`today-hit-found-${hit}`);
    expect(found).toHaveTextContent("Found on their own job board today at");
    expect(found).toHaveTextContent("fits 86 · in Pipeline as Found");
    await user.click(screen.getByTestId(`today-hit-open-${hit}`));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/pipeline");
    });
    expect(await screen.findByTestId("peek")).toBeInTheDocument();
    await waitFor(() => {
      expect(router.state.location.search).toEqual({});
    });
  });

  it("sets a job from a watched company aside with Not for me", async () => {
    const hit = "0190a1b2-0000-7000-8000-0000000000e2";
    const requests = signedIn(
      {
        body: todayAnswer("review_matches", {
          company_hits: [
            {
              id: hit,
              company: "Northwind Stays",
              title: "Staff Engineer",
              found_at: new Date().toISOString(),
            },
          ],
        }),
      },
      {
        "GET /api/v1/companies": { body: companies([], []) },
        [`PATCH /api/v1/opportunities/${hit}`]: { body: opportunity({ id: hit, dismissed: true }) },
      },
    );
    const user = userEvent.setup();
    renderApp("/");
    await user.click(await screen.findByTestId(`today-hit-dismiss-${hit}`));
    await waitFor(() => {
      expect(requests.map(({ route }) => route)).toContain(`PATCH /api/v1/opportunities/${hit}`);
    });
  });

  it("suggests a company to watch, with why, and watches it on a click", async () => {
    const requests = signedIn(
      { body: todayAnswer("none", { latest_search: search() }) },
      {
        "GET /api/v1/companies": { body: companies([suggestion()], []) },
        [`POST /api/v1/companies/${FABRIKAM}/watch`]: {
          status: 202,
          body: companies(
            [],
            [watching({ id: FABRIKAM, name: "Fabrikam Pay", board: "none", finding: true })],
          ),
        },
      },
    );
    const user = userEvent.setup();
    const { router } = renderApp("/");
    const card = await screen.findByTestId("today-suggestion");
    expect(card).toHaveTextContent("Fabrikam Pay: watch their job board?");
    expect(card).toHaveTextContent("3 of their jobs fit you in the last 6 weeks");
    expect(screen.queryByTestId("today-suggestion-reasons")).toBeNull();
    await user.click(screen.getByTestId("today-suggestion-why"));
    expect(screen.getByTestId("today-suggestion-reasons")).toHaveTextContent(
      "Remote in every posting",
    );
    await user.click(screen.getByTestId("today-suggestion-watch"));
    expect(await screen.findByTestId("today-suggestion-watching")).toHaveTextContent(
      "Watching Fabrikam Pay.",
    );
    expect(requests.map(({ route }) => route)).toContain(
      `POST /api/v1/companies/${FABRIKAM}/watch`,
    );
    await user.click(screen.getByTestId("today-suggestion-see"));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/companies");
    });
    expect(router.state.location.search).toEqual({ tab: "watching" });
  });

  it("shows what the AI cost this month on Profile & goals", async () => {
    signedIn({ body: todayAnswer("none") });
    renderApp("/profile");
    expect(await screen.findByTestId("spend-line")).toHaveTextContent("AI this month: €0.42 of €3");
  });
});
