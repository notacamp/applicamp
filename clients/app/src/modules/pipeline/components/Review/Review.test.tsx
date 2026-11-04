import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Schema } from "~/lib/api";

import {
  cv,
  cvDraft,
  cvLine,
  letter,
  letterDraft,
  opportunity,
} from "../../../../../test/pipeline";
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
  const onLetter = vi.fn();
  const view = renderWithProviders(
    <ReviewPage
      id={ID}
      onBack={onBack}
      onPipeline={onPipeline}
      onLetter={onLetter}
      spend={<p data-testid="month-spend">AI this month</p>}
    />,
  );
  return { requests, onBack, onPipeline, onLetter, ...view };
}

/** The draft with the bad number fixed, and so the skill: nothing waits. */
function fixedDraft(): Schema<"CvDraftView"> {
  const draft = cvDraft();
  const role = draft.experience[0];
  return {
    ...draft,
    flagged: 0,
    first_flagged: null,
    can_approve: true,
    waiting: [],
    experience: role === undefined ? [] : [{ ...role, bullets: role.bullets.slice(0, 2) }],
    skills: [{ id: "l6", name: "Rust", flags: [] }],
  };
}

function fixed(): CvAnswer {
  return cv({ draft: fixedDraft() });
}

/** Opens "Every line and where it comes from" under the PDF. */
async function openLines(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByTestId("cv-lines-toggle"));
}

describe("Review the CV", () => {
  it("shows the draft as its PDF first, what waits on the person as questions, and keeps Approve off while one is open", async () => {
    renderReview({ [CV]: { body: cv({ draft: cvDraft() }) } });
    // The PDF, its length and language, and Download draft.
    expect(await screen.findByTestId("cv-preview")).toBeVisible();
    expect(screen.getByTestId("cv-preview-facts")).toHaveTextContent("1 page · English");
    expect(screen.getByTestId("cv-preview-download")).toHaveAttribute(
      "href",
      `/api/v1/opportunities/${ID}/cv/drafts/1/preview.pdf?download=1`,
    );
    // What the app did, and what waits on the person.
    expect(screen.getByTestId("cv-checked")).toHaveTextContent(
      "Checked every line against your profile · fixed 1 line",
    );
    expect(screen.getByTestId("cv-needs-you")).toHaveTextContent("Needs you · 2");
    expect(screen.getByTestId("cv-question-text-l5")).toHaveTextContent(
      "Is this true? “Cut invoice runs to 20 minutes”",
    );
    // The app's question, where the line comes from, and what a yes keeps, word for word.
    expect(screen.getByTestId("cv-question-ask-l5")).toHaveTextContent(
      "Did invoice runs drop to 20 minutes?",
    );
    expect(screen.getByTestId("cv-question-where-l5")).toHaveTextContent(
      "Your profile says about Example Org: “Cut invoice runs from 6 hours to 40 minutes”.",
    );
    expect(screen.getByTestId("cv-question-keeps-l5")).toHaveTextContent(
      "If you keep it, this goes on your profile: “I cut invoice runs to 20 minutes.”",
    );
    expect(screen.getByTestId("cv-question-yes-l5")).toHaveTextContent("Yes, keep it");
    expect(screen.getByTestId("cv-question-change-l5")).toHaveTextContent("Change it");
    expect(screen.getByTestId("cv-question-remove-l5")).toHaveTextContent("Remove it");
    expect(screen.getByTestId("cv-question-text-l7")).toHaveTextContent(
      "Do you have this skill? Kubernetes",
    );
    expect(screen.getByTestId("cv-question-keeps-l7")).toHaveTextContent(
      "If you keep it, it is added to your skills.",
    );
    expect(screen.queryByTestId("cv-question-where-l7")).toBeNull();
    expect(screen.queryByTestId("cv-question-change-l7")).toBeNull();
    // Approve is off, and says what turns it on.
    expect(screen.getByTestId("cv-approve")).toBeDisabled();
    expect(screen.getByTestId("cv-approve")).toHaveTextContent("Approve v1");
    expect(screen.getByTestId("cv-approve-reason")).toHaveTextContent(
      "Answer the 2 questions to approve.",
    );
    expect(screen.getByTestId("cv-draft-number")).toHaveTextContent("Draft 1");
    expect(screen.getByTestId("cv-status")).toHaveTextContent("Draft");
    // The keywords, what changed and the spend.
    expect(screen.getByTestId("cv-keywords-count")).toHaveTextContent("1/2");
    expect(screen.getByTestId("cv-keywords-not_in_profile")).toHaveTextContent(
      "Kafka · nice to have",
    );
    expect(screen.getByTestId("cv-changes")).toHaveTextContent("2 lines reworded");
    expect(screen.getByTestId("cv-drafts-today")).toHaveTextContent("Drafts today: 0 of 20");
    expect(screen.getByTestId("month-spend")).toBeVisible();

    // Every line and where it comes from, one click away.
    const user = userEvent.setup();
    await openLines(user);
    expect(screen.getByTestId("cv-paper")).toBeVisible();
    expect(screen.getByTestId("cv-name")).toHaveTextContent("Alex Demo");
    expect(screen.getByTestId("cv-contact")).toHaveTextContent(
      "alex@example.com · Porto, Portugal",
    );
    expect(screen.getByTestId("cv-line-mark-l3")).toHaveTextContent("Reworded");
    expect(screen.getByTestId("cv-line-mark-l4")).toHaveTextContent("As in profile");
    expect(screen.getByTestId("cv-from-toggle-l3")).toHaveTextContent("From your profile");
    expect(screen.getByTestId("cv-flag-l5")).toHaveTextContent(
      "“20” isn’t in the sources this line cites.",
    );
    expect(screen.getByTestId("cv-left-out")).toHaveTextContent("Left out for this job · 1 item");
    expect(screen.getByTestId("cv-put-back-role-2")).toBeInTheDocument();
  });

  it("Yes, keep it keeps the fact the app proposed, and Remove it removes a skill", async () => {
    let kept: unknown;
    let removed: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`POST ${LINES}/l5/confirm`]: async (request) => {
        kept = await request.json();
        return { body: cv({ draft: { ...cvDraft(), waiting: cvDraft().waiting.slice(1) } }) };
      },
      [`PATCH ${LINES}/l7`]: async (request) => {
        removed = await request.json();
        return { body: fixed() };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-question-yes-l5"));
    await waitFor(() => {
      expect(kept).toEqual({ proposal: true });
    });
    await waitFor(() => {
      expect(screen.queryByTestId("cv-question-l5")).toBeNull();
    });
    await user.click(screen.getByTestId("cv-question-remove-l7"));
    await waitFor(() => {
      expect(removed).toEqual({ remove: true });
    });
    // Nothing waits: Approve is on.
    await waitFor(() => {
      expect(screen.getByTestId("cv-approve")).toBeEnabled();
    });
    expect(screen.getByTestId("cv-approve-ready")).toHaveTextContent(
      "Approve when you’re happy with it.",
    );
    expect(screen.queryByTestId("cv-needs-you")).toBeNull();
  });

  it("Change it opens the line, and sends the new text", async () => {
    let sent: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`PATCH ${LINES}/l5`]: async (request) => {
        sent = await request.json();
        return { body: cv({ draft: cvDraft() }) };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-question-change-l5"));
    const input = screen.getByTestId("cv-question-input-l5");
    // It starts from the person's own words, never the model's.
    expect(input).toHaveValue("Cut invoice runs from 6 hours to 40 minutes");
    await user.clear(input);
    // An empty line is never saved.
    expect(screen.getByTestId("cv-question-save-l5")).toBeDisabled();
    await user.type(input, "Cut invoice runs from 6 hours to 40 minutes");
    await user.click(screen.getByTestId("cv-question-save-l5"));
    await waitFor(() => {
      expect(sent).toEqual({ text: "Cut invoice runs from 6 hours to 40 minutes" });
    });
  });

  it("adds a skill the person has with Yes", async () => {
    let body: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`POST ${LINES}/l7/confirm`]: async (request) => {
        body = await request.json();
        return { body: fixed() };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-question-yes-l7"));
    await waitFor(() => {
      expect(body).toEqual({ proposal: true });
    });
  });

  it("makes the PDF again in another layout, with no AI", async () => {
    let asked: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`POST /api/v1/opportunities/${ID}/cv/drafts/1/preview`]: async (request) => {
        asked = await request.json();
        return { body: cv({ draft: cvDraft() }) };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cv-template-compact"));
    await waitFor(() => {
      expect(asked).toEqual({ template: "compact", length: 1 });
    });
  });

  it("still saves the person's own words as a fact from the lines (It's true)", async () => {
    let said: unknown;
    renderReview({
      [CV]: { body: cv({ draft: cvDraft() }) },
      [`POST ${LINES}/l5/confirm`]: async (request) => {
        said = await request.json();
        return { body: cv({ draft: cvDraft() }) };
      },
    });
    const user = userEvent.setup();
    await openLines(user);
    await user.click(screen.getByTestId("cv-confirm-l5"));
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
  });

  it("lets the person undo an approval for a moment, saying what starts next and its cost", async () => {
    let undone = false;
    const later = new Date(Date.now() + 60_000).toISOString();
    renderReview({
      [CV]: {
        body: cv({
          draft: { ...fixedDraft(), status: "approved", version: 1, undo_until: later },
          next: { step: "letter", eur: 0.002, at: later },
        }),
      },
      [`POST /api/v1/opportunities/${ID}/cv/drafts/1/unapprove`]: () => {
        undone = true;
        return { body: fixed() };
      },
    });
    expect(await screen.findByTestId("cv-approved-now")).toHaveTextContent("Approved");
    expect(screen.getByTestId("cv-next")).toHaveTextContent(
      "Writing your cover letter next, about €0.002. Undo stops it.",
    );
    await userEvent.setup().click(screen.getByTestId("cv-undo"));
    await waitFor(() => {
      expect(undone).toBe(true);
    });
    expect(await screen.findByTestId("cv-approve")).toBeEnabled();
  });

  it("says what the job wants, what the app did and what it cost, and what it rewrote", async () => {
    const draft = cvDraft();
    const role = draft.experience[0];
    const fixedLine = cvLine("l8", "Led a team of 4 engineers", {
      fixed_by_app: true,
      original: "Led a team of 4 engineers building ledgers for small shops",
    });
    renderReview({
      [CV]: {
        body: cv({
          draft: {
            ...draft,
            experience:
              role === undefined ? [] : [{ ...role, bullets: [...role.bullets, fixedLine] }],
          },
          suggested_language: "fr",
        }),
      },
    });
    expect(await screen.findByTestId("cv-wants")).toHaveTextContent("What the job wants");
    expect(screen.getByTestId("cv-want-1")).toHaveTextContent(
      "Kafka · nice to have · not on your profile",
    );
    expect(screen.getByTestId("cv-done")).toHaveTextContent("Done by the app · €0.006");
    expect(screen.getByTestId("cv-done-draft-1")).toHaveTextContent("Wrote the CV · €0.004");
    expect(screen.getByTestId("cv-done-pdf")).toHaveTextContent("Made the PDF");
    expect(screen.getByTestId("cv-fixed-l8")).toHaveTextContent(
      "It said: “Led a team of 4 engineers building ledgers for small shops”",
    );
    // Style, Length, Language: the CV's and the posting's.
    expect(screen.getByTestId("cv-layout-language")).toHaveTextContent("Style");
    expect(screen.getByTestId("cv-language-en")).toBeVisible();
    await userEvent.setup().click(screen.getByTestId("cv-language-fr"));
    expect(screen.getByTestId("cv-language-next")).toHaveTextContent("Press Write it again");
    expect(screen.getByTestId("cv-make-again")).toHaveTextContent("Write it again");
    expect(screen.getByTestId("cv-make-again")).not.toHaveTextContent("€");
    expect(screen.getByTestId("cv-write-again-cost")).toHaveTextContent(
      "Writing it again costs about €0.01.",
    );
  });

  it("says when the PDF could not be made, keeps the text and tries again", async () => {
    let asked: unknown;
    renderReview({
      [CV]: {
        body: cv({
          draft: cvDraft({
            preview: {
              ready: false,
              stale: false,
              error: "cv_pdf_unavailable",
              at: "2026-10-31T10:00:01Z",
            },
          }),
        }),
      },
      [`POST /api/v1/opportunities/${ID}/cv/drafts/1/preview`]: async (request) => {
        asked = await request.json();
        return { body: cv({ draft: cvDraft() }) };
      },
    });
    expect(await screen.findByTestId("cv-preview-error")).toHaveTextContent(
      "Could not make the PDF for Example Co. The text of the CV is saved.",
    );
    const text = screen.getByTestId("cv-preview-text");
    expect(text).toHaveAttribute("download", "CV - Example Co.txt");
    expect(decodeURIComponent(text.getAttribute("href") ?? "")).toContain(
      "- Led a team of 4 engineers",
    );
    await userEvent.setup().click(screen.getByTestId("cv-preview-retry"));
    await waitFor(() => {
      expect(asked).toEqual({ template: "clean", length: 1 });
    });
  });

  it("says when the company's site asks apps not to read it, and reads what the person pastes", async () => {
    let pasted: unknown;
    renderReview({
      [CV]: {
        body: cv({
          draft: cvDraft({ company_facts: 0 }),
          company_read_failure: "robots_disallows",
        }),
      },
      [`POST /api/v1/opportunities/${ID}/brief/paste`]: async (request) => {
        pasted = await request.json();
        return { status: 200, body: {} };
      },
    });
    expect(await screen.findByTestId("cv-site")).toHaveTextContent(
      "The site asks apps not to read it. The CV was written from the job posting and your profile only.",
    );
    expect(screen.getByTestId("cv-site-paste")).not.toHaveTextContent("€");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("cv-site-paste"));
    await user.type(screen.getByTestId("cv-site-text"), "Example Co builds ledgers.");
    await user.click(screen.getByTestId("cv-site-read"));
    await waitFor(() => {
      expect(pasted).toEqual({ text: "Example Co builds ledgers." });
    });
    expect(await screen.findByTestId("cv-site-done")).toHaveTextContent(
      "Write it again to use what the site says.",
    );
  });

  it("waits while the app writes the CV, with no button to press", async () => {
    renderReview({ [CV]: { body: cv({ writing: true }) } });
    expect(await screen.findByTestId("cv-writing")).toHaveTextContent(
      "Writing your CV for Example Co. Usually under a minute.",
    );
    expect(screen.queryByTestId("cv-draft")).toBeNull();
  });

  it("says a quote is what the person told the app, or what their profile says", async () => {
    const draft = cvDraft();
    const [first, ...rest] = draft.waiting;
    if (first === undefined) {
      throw new Error("the fixture waits on something");
    }
    renderReview({
      [CV]: {
        body: cv({
          draft: {
            ...draft,
            waiting: [
              { ...first, quote: "I owned invoicing end to end", quote_from: "you" },
              ...rest,
            ],
          },
        }),
      },
    });
    expect(await screen.findByTestId(`cv-question-where-${first.line_id}`)).toHaveTextContent(
      "You told us about Example Org: “I owned invoicing end to end”.",
    );
    // Change it starts from those words, either way.
    await userEvent.setup().click(screen.getByTestId(`cv-question-change-${first.line_id}`));
    expect(screen.getByTestId(`cv-question-input-${first.line_id}`)).toHaveValue(
      "I owned invoicing end to end",
    );
  });

  it("says why a line can't be kept: another job's, or too long", async () => {
    renderReview({
      [CV]: {
        body: cv({
          draft: cvDraft({
            waiting: [
              {
                line_id: "l3",
                kind: "line",
                text: "Built ingestion for 3 partner feeds",
                proposal: null,
                cannot_keep: "other_position",
                flags: [{ kind: "other_position", cites: ["w1.h1"] }],
              },
              {
                line_id: "l4",
                kind: "line",
                text: "Rebuilt billing",
                proposal: null,
                cannot_keep: "too_long",
                flags: [{ kind: "claim_not_in_sources", claim: "12", cites: ["w0.h1"] }],
              },
            ],
          }),
        }),
      },
    });
    expect(await screen.findByTestId("cv-question-keeps-l3")).toHaveTextContent(
      "This line is about another job on your profile. Change it or remove it.",
    );
    expect(screen.getByTestId("cv-question-keeps-l4")).toHaveTextContent(
      "This line is too long to keep on your profile as it is. Make it shorter or remove it.",
    );
    expect(screen.queryByTestId("cv-question-yes-l3")).toBeNull();
    expect(screen.queryByTestId("cv-question-yes-l4")).toBeNull();
    expect(screen.getByTestId("cv-question-change-l4")).toBeVisible();
  });

  it("offers Make the PDF when the PDF is older than the last change, or there is none", async () => {
    const asked: unknown[] = [];
    const draft = cvDraft();
    const stale = { ...draft, preview: draft.preview ? { ...draft.preview, stale: true } : null };
    renderReview({
      [CV]: { body: cv({ draft: stale }) },
      [`POST /api/v1/opportunities/${ID}/cv/drafts/1/preview`]: async (request) => {
        asked.push(await request.json());
        return { body: cv({ draft: cvDraft() }) };
      },
    });
    const user = userEvent.setup();
    expect(await screen.findByTestId("cv-preview-stale")).toHaveTextContent(
      "The PDF is older than your last change.",
    );
    await user.click(screen.getByTestId("cv-preview-make"));
    await waitFor(() => {
      expect(asked).toEqual([{ template: "clean", length: 1 }]);
    });
    await waitFor(() => {
      expect(screen.queryByTestId("cv-preview-stale")).toBeNull();
    });
    expect(screen.getByTestId("cv-preview-download")).toBeVisible();
  });

  it("offers Make the PDF, never an endless wait, when a draft has none", async () => {
    renderReview({ [CV]: { body: cv({ draft: cvDraft({ preview: null }) }) } });
    expect(await screen.findByTestId("cv-preview-none")).toHaveTextContent(
      "This draft has no PDF yet.",
    );
    expect(screen.getByTestId("cv-preview-make")).toHaveTextContent("Make the PDF");
  });

  it("marks a line the app rewrote", async () => {
    const draft = fixedDraft();
    const role = draft.experience[0];
    const first = role?.bullets[0];
    if (role === undefined || first === undefined) {
      throw new Error("the fixture has a bullet");
    }
    const rewritten = {
      ...draft,
      experience: [
        {
          ...role,
          bullets: [
            { ...first, fixed_by_app: true, original: "Led a team of 12" },
            ...role.bullets.slice(1),
          ],
        },
      ],
    };
    renderReview({ [CV]: { body: cv({ draft: rewritten }) } });
    const user = userEvent.setup();
    await openLines(user);
    expect(await screen.findByTestId(`cv-line-mark-${first.id}`)).toHaveTextContent(
      "Fixed by the app",
    );
  });

  it("offers to make the PDF of a CV approved without one", async () => {
    let asked: unknown;
    renderReview({
      [CV]: {
        body: cv({
          draft: {
            ...fixedDraft(),
            status: "approved",
            version: 1,
            can_approve: false,
            needs_version: true,
            preview: null,
          },
        }),
      },
      [`POST /api/v1/opportunities/${ID}/cv/versions`]: async (request) => {
        asked = await request.json();
        return { body: fixed() };
      },
    });
    const user = userEvent.setup();
    expect(await screen.findByTestId("cv-needs-version")).toHaveTextContent(
      "This CV was approved but has no PDF yet",
    );
    await user.click(screen.getByTestId("cv-make-pdf"));
    await waitFor(() => {
      expect(asked).toEqual({ template: "clean", length: 1 });
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
    expect(screen.getByTestId("cv-checked")).toBeVisible();
    await user.click(approve);
    await waitFor(() => {
      expect(screen.getByTestId("cv-status")).toHaveTextContent("Approved · v1");
    });
    expect(screen.getByTestId("cv-banner-approved")).toBeVisible();
    expect(screen.queryByTestId("cv-approve")).toBeNull();
    expect(screen.queryByTestId("cv-edit-l4")).toBeNull();
  });

  it("offers the cover letter once the CV is approved, and opens it after writing", async () => {
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
    const { onLetter } = renderReview({
      [CV]: { body: approved },
      [`GET /api/v1/opportunities/${ID}/letter`]: { body: letter() },
      [`POST /api/v1/opportunities/${ID}/letter/drafts`]: {
        body: letter({ draft: letterDraft() }),
      },
    });
    const write = await screen.findByTestId("cv-letter-write");
    expect(write).toHaveTextContent("Write the cover letter");
    expect(screen.getByTestId("cv-letter-write-cost")).toHaveTextContent("about €0.002");
    await userEvent.setup().click(write);
    await waitFor(() => {
      expect(onLetter).toHaveBeenCalledWith(ID);
    });
  });

  it("has no cover letter before the CV is approved", async () => {
    renderReview({ [CV]: { body: fixed() } });
    expect(await screen.findByTestId("cv-approve")).toBeVisible();
    expect(screen.queryByTestId("cv-letter")).toBeNull();
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
    await openLines(user);
    await user.click(screen.getByTestId("cv-put-back-role-2"));
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
    await user.click(await screen.findByTestId("cv-question-remove-l5"));
    expect(await screen.findByTestId("cv-error")).toHaveTextContent(
      "This draft was approved or put away, so it can’t change. Make the CV again.",
    );
  });

  it("offers the draft when there is none, and a company fact used as an achievement can't be confirmed", async () => {
    renderReview({ [CV]: { body: cv() } });
    expect(await screen.findByTestId("cv-empty")).toBeVisible();
    expect(screen.getByTestId("cv-draft")).toHaveTextContent("Draft the CV");
    expect(screen.getByTestId("cv-draft-cost")).toHaveTextContent(
      "Drafting the CV costs about €0.01.",
    );
  });

  it("offers only Change it and Remove it for a company fact outside the summary", async () => {
    const flags: Schema<"Reason">[] = [
      { kind: "company_fact_outside_summary", cites: ["cab12cd34"] },
    ];
    const line = cvLine("l9", "Builds ledgers for small shops", { flags });
    const draft = cvDraft();
    const role = draft.experience[0];
    renderReview({
      [CV]: {
        body: cv({
          draft: {
            ...draft,
            experience: role === undefined ? [] : [{ ...role, bullets: [line] }],
            waiting: [
              {
                line_id: "l9",
                kind: "line",
                text: "Builds ledgers for small shops",
                proposal: null,
                role: "Senior Engineer · Example Org",
                flags,
              },
            ],
          },
        }),
      },
    });
    expect(await screen.findByTestId("cv-question-keeps-l9")).toHaveTextContent(
      "This is about the company, not about you. Change it or remove it.",
    );
    expect(screen.queryByTestId("cv-question-yes-l9")).toBeNull();
    expect(screen.getByTestId("cv-question-change-l9")).toBeVisible();
    expect(screen.getByTestId("cv-question-remove-l9")).toBeVisible();
    const user = userEvent.setup();
    await openLines(user);
    expect(screen.getByTestId("cv-edit-l9")).toBeVisible();
    expect(screen.queryByTestId("cv-confirm-l9")).toBeNull();
    expect(screen.getByTestId("cv-flag-l9")).toHaveTextContent(
      "Company facts can’t be your achievements",
    );
  });
});
