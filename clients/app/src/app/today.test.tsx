import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { estimate, search, todayAnswer } from "../../test/pipeline";
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

  it("shows what the AI cost this month on Profile & goals", async () => {
    signedIn({ body: todayAnswer("none") });
    renderApp("/profile");
    expect(await screen.findByTestId("spend-line")).toHaveTextContent("AI this month: €0.42 of €3");
  });
});
