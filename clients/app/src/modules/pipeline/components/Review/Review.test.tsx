import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Schema } from "~/lib/api";

import { cv, cvDraft, cvLine, opportunity } from "../../../../../test/pipeline";
import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { ReviewPage } from "./ReviewPage";

const ID = "0190a1b2-0000-7000-8000-000000000001";
const CV = `GET /api/v1/opportunities/${ID}/cv`;
const LINES = `/api/v1/opportunities/${ID}/cv/drafts/1/lines`;

type CvAnswer = Schema<"Cv">;

function renderReview(routes: Record<string, Stub>) {
  const requests = stubApi({
    [`GET /api/v1/opportunities/${ID}`]: { body: opportunity({ id: ID, stage: "shortlisted" }) },
    ...routes,
  });
  const onBack = vi.fn();
  const onPipeline = vi.fn();
  const view = renderWithProviders(
    <ReviewPage
      id={ID}
      onBack={onBack}
      onPipeline={onPipeline}
      spend={<p data-testid="month-spend">AI this month</p>}
    />,
  );
  return { requests, onBack, onPipeline, ...view };
}

/** The draft with the bad number fixed, and so the skill. */
function fixedDraft(): Schema<"CvDraftView"> {
  const draft = cvDraft();
  const role = draft.experience[0];
  return {
    ...draft,
    flagged: 0,
    first_flagged: null,
    can_approve: true,
    experience: role === undefined ? [] : [{ ...role, bullets: role.bullets.slice(0, 2) }],
    skills: [{ id: "l6", name: "Rust", flags: [] }],
  };
}

function fixed(): CvAnswer {
  return cv({ draft: fixedDraft() });
}

describe("Review the CV", () => {
  it("shows the CV as a page with the contact the server filled in, each line marked, and blocks Approve while a line is flagged", async () => {
    renderReview({ [CV]: { body: cv({ draft: cvDraft() }) } });
    expect(await screen.findByTestId("cv-paper")).toBeVisible();
    expect(screen.getByTestId("cv-name")).toHaveTextContent("Alex Demo");
    expect(screen.getByTestId("cv-contact")).toHaveTextContent(
      "alex@example.com · Porto, Portugal",
    );
    expect(screen.getByTestId("cv-line-mark-l3")).toHaveTextContent("Reworded");
    expect(screen.getByTestId("cv-line-mark-l4")).toHaveTextContent("As in profile");
    expect(screen.getByTestId("cv-draft-number")).toHaveTextContent("Draft 1");
    expect(screen.getByTestId("cv-status")).toHaveTextContent("Draft");
    // A reworded line opens onto the profile's own words.
    expect(screen.getByTestId("cv-from-toggle-l3")).toHaveTextContent("From your profile");
    // The flagged ones say why.
    expect(screen.getByTestId("cv-flag-l5")).toHaveTextContent(
      "“20” isn’t in the sources this line cites.",
    );
    expect(screen.getByTestId("cv-line-l5")).toHaveAttribute("data-flagged");
    expect(screen.getByTestId("cv-skill-flag-l7")).toHaveTextContent(
      "Kubernetes isn’t one of your skills.",
    );
    // Approve is off, and says when it turns on.
    expect(screen.getByTestId("cv-approve")).toBeDisabled();
    expect(screen.getByTestId("cv-approve")).toHaveTextContent("Approve v1");
    expect(screen.getByTestId("cv-approve-reason")).toBeVisible();
    // The aside: the banner, the keywords, what changed and the spend.
    expect(screen.getByTestId("cv-banner-flagged")).toHaveTextContent(
      "2 lines can’t be traced to your profile",
    );
    expect(screen.getByTestId("cv-keywords-count")).toHaveTextContent("1/2");
    expect(screen.getByTestId("cv-keywords-not_in_profile")).toHaveTextContent(
      "Kafka · nice to have",
    );
    expect(screen.getByTestId("cv-changes")).toHaveTextContent("2 lines reworded");
    expect(screen.getByTestId("cv-drafts-today")).toHaveTextContent("Drafts today: 0 of 20");
    expect(screen.getByTestId("month-spend")).toBeVisible();
    // Left out, collapsed, with Put back.
    expect(screen.getByTestId("cv-left-out")).toHaveTextContent("Left out for this job · 1 item");
    expect(screen.getByTestId("cv-put-back-role-2")).toBeInTheDocument();
  });

  it("fixes a flagged line by editing it, and sends the new text", async () => {
    let sent: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`PATCH ${LINES}/l5`]: async (request) => {
        sent = await request.json();
        return { body: cv({ draft: cvDraft() }) };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-edit-l5"));
    const input = screen.getByTestId("cv-edit-input-l5");
    await user.clear(input);
    await user.type(input, "Cut invoice runs from 6 hours to 40 minutes");
    await user.click(screen.getByTestId("cv-edit-save-l5"));
    await waitFor(() => {
      expect(sent).toEqual({ text: "Cut invoice runs from 6 hours to 40 minutes" });
    });
  });

  it("saves the person's own words as a fact (It's true) and removes a skill", async () => {
    let said: unknown;
    let removed: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`POST ${LINES}/l5/confirm`]: async (request) => {
        said = await request.json();
        return { body: cv({ draft: cvDraft() }) };
      },
      [`PATCH ${LINES}/l7`]: async (request) => {
        removed = await request.json();
        return { body: fixed() };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-confirm-l5"));
    expect(screen.getByTestId("cv-confirm-input-l5")).toBeVisible();
    expect(screen.getByTestId("cv-confirm-input-l5")).toHaveAccessibleDescription(
      "This saves your words to Senior Engineer · Example Org in your profile, then checks the line again.",
    );
    await user.type(
      screen.getByTestId("cv-confirm-input-l5"),
      "I cut invoice runs to 20 minutes in 2023",
    );
    await user.click(screen.getByTestId("cv-confirm-save-l5"));
    await waitFor(() => {
      expect(said).toEqual({ text: "I cut invoice runs to 20 minutes in 2023" });
    });
    await user.click(screen.getByTestId("cv-skill-remove-l7"));
    await waitFor(() => {
      expect(removed).toEqual({ remove: true });
    });
    // The CV now answered with nothing flagged: Approve is on.
    await waitFor(() => {
      expect(screen.getByTestId("cv-approve")).toBeEnabled();
    });
    expect(screen.getByTestId("cv-banner-clean")).toHaveTextContent(
      "Every line traces to your profile",
    );
  });

  it("adds a skill the person has to their profile, with no words to write", async () => {
    let body: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`POST ${LINES}/l7/confirm`]: async (request) => {
        body = await request.json();
        return { body: fixed() };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-skill-confirm-l7"));
    await waitFor(() => {
      expect(body).toEqual({});
    });
  });

  it("approves once nothing is flagged, then reads as a version that can't change", async () => {
    const approved = cv({
      draft: { ...fixedDraft(), status: "approved", version: 1, can_approve: false },
      versions: [
        {
          n: 1,
          version: 1,
          language: "en",
          approved_at: "2026-10-31T11:00:00Z",
          created_at: "2026-10-31T11:00:00Z",
          template: "clean",
          length: 1,
          pages: 1,
          size_bytes: 30_000,
          current: true,
          checks: {
            sections: { expected: ["Summary"], missing: [] },
            keywords: { total: 1, found: 1, missing: [] },
            lines: { total: 3, missing: [] },
            text_layer: true,
            contact: { name: true, email: true, links: true },
            size: { pages: 1, length: 1, bytes: 30_000 },
          },
        },
      ],
    });
    renderReview({
      [CV]: { body: fixed() },
      [`POST /api/v1/opportunities/${ID}/cv/drafts/1/approve`]: { body: approved },
    });
    const user = userEvent.setup();
    const approve = await screen.findByTestId("cv-approve");
    expect(approve).toBeEnabled();
    expect(screen.getByTestId("cv-banner-clean")).toBeVisible();
    await user.click(approve);
    await waitFor(() => {
      expect(screen.getByTestId("cv-status")).toHaveTextContent("Approved · v1");
    });
    expect(screen.getByTestId("cv-banner-approved")).toBeVisible();
    expect(screen.queryByTestId("cv-approve")).toBeNull();
    expect(screen.queryByTestId("cv-edit-l4")).toBeNull();
  });

  it("puts back what was left out, as the profile has it", async () => {
    let body: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`POST /api/v1/opportunities/${ID}/cv/drafts/1/put-back`]: async (request) => {
        body = await request.json();
        return { body: fixed() };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-put-back-role-2"));
    await waitFor(() => {
      expect(body).toEqual({ position: 2 });
    });
  });

  it("says in plain words why a change was refused", async () => {
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`PATCH ${LINES}/l5`]: {
        status: 409,
        body: {
          type: "about:blank",
          title: "Conflict",
          status: 409,
          detail: "x",
          code: "draft_frozen",
        },
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-remove-l5"));
    expect(await screen.findByTestId("cv-error")).toHaveTextContent(
      "This draft was approved or put away, so it can’t change. Make the CV again.",
    );
  });

  it("offers the draft when there is none, and a company fact used as an achievement can't be confirmed", async () => {
    renderReview({ [CV]: { body: cv() } });
    expect(await screen.findByTestId("cv-empty")).toBeVisible();
    expect(screen.getByTestId("cv-draft")).toHaveTextContent("Draft the CV · about €0.002");
  });

  it("offers only Edit and Remove for a company fact outside the summary", async () => {
    const line = cvLine("l9", "Builds ledgers for small shops", {
      flags: [{ kind: "company_fact_outside_summary", cites: ["cab12cd34"] }],
    });
    const draft = cvDraft();
    const role = draft.experience[0];
    renderReview({
      [CV]: {
        body: cv({
          draft: {
            ...draft,
            experience: role === undefined ? [] : [{ ...role, bullets: [line] }],
          },
        }),
      },
    });
    expect(await screen.findByTestId("cv-edit-l9")).toBeVisible();
    expect(screen.getByTestId("cv-remove-l9")).toBeVisible();
    expect(screen.queryByTestId("cv-confirm-l9")).toBeNull();
    expect(screen.getByTestId("cv-flag-l9")).toHaveTextContent(
      "Company facts can’t be your achievements",
    );
  });
});
