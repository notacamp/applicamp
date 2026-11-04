import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { renderWithProviders, stubApi } from "../../../../../test/render";
import {
  FACT_PROPOSALS_QUERY_KEY,
  type FactProposal,
  type FactProposalsView,
} from "../../api/facts";

import { CompletePage } from "./CompletePage";
import { FactsNudge } from "./FactsNudge";

const LEAD = "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a01";
const SITE = "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a02";
const SKILL = "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a03";

/** Invented facts about an invented person. */
function fact(id: string, text: string, rest: Partial<FactProposal>): FactProposal {
  return {
    id,
    text,
    quote: text,
    source: "you_said",
    basis: null,
    area: "results",
    state: "open",
    decided_at: null,
    undo_until: null,
    ...rest,
  };
}

function view(states: Record<string, FactProposal["state"]> = {}): FactProposalsView {
  const facts = [
    fact(LEAD, "Led 4 engineers", { quote: "I led 4 engineers" }),
    fact(SITE, "Built an open-source billing toolkit", {
      source: "your_website",
      basis: "https://alex.example.com/projects",
    }),
    fact(SKILL, "Elixir", { quote: "I use Rails, Elixir and Postgres" }),
  ].map((f) => {
    const state = states[f.id] ?? "open";
    return state === "open"
      ? f
      : {
          ...f,
          state,
          decided_at: "2026-11-04T10:00:00Z",
          undo_until: "2026-11-05T10:00:00Z",
        };
  });
  const count = (state: FactProposal["state"]) => facts.filter((f) => f.state === state).length;
  return {
    groups: [
      {
        target: "position",
        position: 0,
        title: "Lead Engineer",
        company: "Example Co",
        start_date: "2021-03",
        end_date: null,
        proposals: facts.slice(0, 2),
      },
      {
        target: "skills",
        position: null,
        title: null,
        company: null,
        start_date: null,
        end_date: null,
        proposals: facts.slice(2),
      },
    ],
    open: count("open"),
    kept: count("kept"),
    dropped: count("dropped"),
    cv_read: {
      applied_at: "2026-11-04T09:00:00Z",
      jobs: 13,
      achievements: 31,
      skills: 14,
      degrees: 1,
      languages: 3,
      certificates: 0,
      projects: 0,
    },
  };
}

const PROPOSALS = "/api/v1/profile/facts/proposals";

describe("Complete your profile", () => {
  it("shows the facts by job with where each comes from, and keeps, drops and undoes them in place", async () => {
    const requests = stubApi({
      [`GET ${PROPOSALS}`]: { body: view() },
      [`POST ${PROPOSALS}/${LEAD}/keep`]: { body: view({ [LEAD]: "kept" }) },
      [`POST ${PROPOSALS}/${SITE}/drop`]: { body: view({ [LEAD]: "kept", [SITE]: "dropped" }) },
      [`POST ${PROPOSALS}/${LEAD}/undo`]: { body: view({ [SITE]: "dropped" }) },
    });
    const user = userEvent.setup();
    renderWithProviders(<CompletePage onBack={vi.fn()} />);

    expect(await screen.findByTestId("complete-cv-read")).toHaveTextContent(
      "Found 13 jobs, 31 achievements, 14 skills, 1 degree, and 3 languages.",
    );
    expect(screen.getByTestId("profile-complete")).toHaveTextContent(
      "We found 3 facts about your work. Keep the right ones.",
    );
    expect(screen.getByTestId("complete-progress")).toHaveTextContent("0 of 3 decided");
    expect(screen.getByTestId("complete-group-0")).toHaveTextContent("Example Co · 2021 – now");
    expect(screen.getByTestId("complete-group-1")).toHaveTextContent("Your skills");
    expect(screen.getByTestId(`complete-fact-${LEAD}`)).toHaveTextContent(
      "you said“I led 4 engineers”",
    );
    expect(screen.getByTestId(`complete-fact-${SITE}`)).toHaveTextContent(
      "from your websitehttps://alex.example.com/projects",
    );
    expect(screen.queryByTestId("complete-done")).toBeNull();

    await user.click(screen.getByTestId(`complete-keep-${LEAD}`));
    const kept = await screen.findByTestId(`complete-undo-${LEAD}`);
    expect(screen.getByTestId(`complete-fact-${LEAD}`)).toHaveTextContent("Kept");
    expect(screen.getByTestId("complete-progress")).toHaveTextContent("1 of 3 decided");

    await user.click(screen.getByTestId(`complete-drop-${SITE}`));
    expect(await screen.findByTestId(`complete-undo-${SITE}`)).toBeInTheDocument();
    expect(screen.getByTestId(`complete-fact-${SITE}`)).toHaveTextContent("Dropped");

    await user.click(kept);
    expect(await screen.findByTestId(`complete-keep-${LEAD}`)).toBeInTheDocument();
    expect(requests.map(({ route }) => route)).toContain(`POST ${PROPOSALS}/${LEAD}/undo`);
    expect(
      requests
        .find(({ route }) => route === `POST ${PROPOSALS}/${LEAD}/keep`)
        ?.headers.get("x-applicamp-csrf"),
    ).toBe("1");
  });

  it("keeps all at once and says how many were added, with a day to undo", async () => {
    stubApi({
      [`GET ${PROPOSALS}`]: { body: view() },
      [`POST ${PROPOSALS}/keep-all`]: {
        body: view({ [LEAD]: "kept", [SITE]: "kept", [SKILL]: "kept" }),
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<CompletePage onBack={vi.fn()} />);
    await user.click(await screen.findByTestId("complete-keep-all"));
    expect(await screen.findByTestId("complete-done")).toHaveTextContent(
      "3 facts added to your profile. You can undo this for a day.",
    );
    expect(screen.getByTestId("complete-keep-all")).toBeDisabled();
  });

  it("says when there is nothing to add, and goes back", async () => {
    stubApi({
      [`GET ${PROPOSALS}`]: {
        body: { groups: [], open: 0, kept: 0, dropped: 0, cv_read: null },
      },
    });
    const user = userEvent.setup();
    const onBack = vi.fn<() => void>();
    renderWithProviders(<CompletePage onBack={onBack} />);
    expect(await screen.findByTestId("complete-empty")).toHaveTextContent("Nothing new to add");
    expect(screen.queryByTestId("complete-cv-read")).toBeNull();
    await user.click(screen.getByTestId("complete-empty-back"));
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("says when a job changed on the profile since", async () => {
    stubApi({
      [`GET ${PROPOSALS}`]: { body: view() },
      [`POST ${PROPOSALS}/${LEAD}/keep`]: {
        status: 409,
        body: { status: 409, code: "profile_changed" },
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<CompletePage onBack={vi.fn()} />);
    await user.click(await screen.findByTestId(`complete-keep-${LEAD}`));
    expect(await screen.findByTestId("complete-error")).toHaveTextContent(
      "That job changed on your profile.",
    );
  });
});

describe("the nudge on Profile & goals", () => {
  it("says how many facts wait, and leads to them; nothing when none wait", async () => {
    stubApi({ [`GET ${PROPOSALS}`]: { body: view() } });
    const user = userEvent.setup();
    const onOpen = vi.fn<() => void>();
    const { unmount } = renderWithProviders(<FactsNudge onOpen={onOpen} />);
    expect(await screen.findByTestId("profile-facts-nudge")).toHaveTextContent(
      "3 new facts for your profile",
    );
    await user.click(screen.getByTestId("profile-facts-open"));
    expect(onOpen).toHaveBeenCalledOnce();
    unmount();

    stubApi({
      [`GET ${PROPOSALS}`]: { body: view({ [LEAD]: "kept", [SITE]: "kept", [SKILL]: "kept" }) },
    });
    const { queryClient } = renderWithProviders(<FactsNudge onOpen={onOpen} />);
    await waitFor(() => {
      expect(queryClient.getQueryState(FACT_PROPOSALS_QUERY_KEY)?.status).toBe("success");
    });
    expect(screen.queryByTestId("profile-facts-nudge")).toBeNull();
  });
});
