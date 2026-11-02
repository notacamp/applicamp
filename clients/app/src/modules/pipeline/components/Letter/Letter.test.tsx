import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Schema } from "~/lib/api";

import { letter, letterDraft, opportunity } from "../../../../../test/pipeline";
import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { LetterPage } from "./LetterPage";

const ID = "0190a1b2-0000-7000-8000-000000000001";
const LETTER = `GET /api/v1/opportunities/${ID}/letter`;
const DRAFTS = `/api/v1/opportunities/${ID}/letter/drafts`;

type LetterAnswer = Schema<"Letter">;

function renderLetter(routes: Record<string, Stub>) {
  const requests = stubApi({
    [`GET /api/v1/opportunities/${ID}`]: { body: opportunity({ id: ID, stage: "shortlisted" }) },
    ...routes,
  });
  const onBack = vi.fn();
  const onPipeline = vi.fn();
  const view = renderWithProviders(
    <LetterPage
      id={ID}
      onBack={onBack}
      onPipeline={onPipeline}
      spend={<p data-testid="month-spend">AI this month</p>}
    />,
  );
  return { requests, onBack, onPipeline, ...view };
}

/** The draft with every open thing dealt with. */
function fixed(): LetterAnswer {
  const draft = letterDraft();
  return letter({
    draft: {
      ...draft,
      flagged: 0,
      first_flagged: null,
      can_approve: true,
      paragraphs: draft.paragraphs.map((paragraph) => ({
        ...paragraph,
        sentences: paragraph.sentences.map((sentence) => ({ ...sentence, flags: [] })),
      })),
      answers: draft.answers.map((answer) => ({
        ...answer,
        flags: [],
        ...(answer.id === "q11" ? { text: "One month.", own: true } : {}),
      })),
    },
  });
}

describe("Review the cover letter", () => {
  it("shows the letter as a page with the greeting and contact the server filled in, each paragraph tagged by what it rests on, and blocks Approve while anything is open", async () => {
    renderLetter({ [LETTER]: { body: letter({ draft: letterDraft() }) } });
    expect(await screen.findByTestId("letter-paper")).toBeVisible();
    expect(screen.getByTestId("letter-name")).toHaveTextContent("Alex Demo");
    expect(screen.getByTestId("letter-contact")).toHaveTextContent(
      "alex@example.com · Porto, Portugal",
    );
    expect(screen.getByTestId("letter-greeting")).toHaveTextContent(
      "Dear hiring team at Example Co,",
    );
    expect(screen.getByTestId("letter-tags-p2")).toHaveTextContent("Company fact");
    expect(screen.getByTestId("letter-tags-p2")).toHaveTextContent("From your profile");
    expect(screen.getByTestId("letter-tags-p9")).toHaveTextContent("No claims");
    expect(screen.getByTestId("letter-from-toggle-p2")).toHaveTextContent("Where this came from");
    expect(screen.getByTestId("letter-draft-number")).toHaveTextContent("Draft 1");
    expect(screen.getByTestId("letter-status")).toHaveTextContent("Draft");
    // The flagged ones say why, and are not dropped.
    expect(screen.getByTestId("letter-flag-s4")).toHaveTextContent(
      "“go-to” isn’t in your profile. Is it true?",
    );
    expect(screen.getByTestId("letter-flag-s6")).toHaveTextContent(
      "cites only a fact about the company",
    );
    expect(screen.getByTestId("letter-paragraph-p5")).toHaveAttribute("data-flagged");
    // The question the profile can't answer says so and has a box.
    expect(screen.getByTestId("letter-answer-flag-q11")).toHaveTextContent(
      "Your profile and goals don’t say. Answer it yourself.",
    );
    expect(screen.getByTestId("letter-answer-input-q11")).toBeVisible();
    expect(screen.getByTestId("letter-answer-text-q10")).toHaveTextContent("Because you build");
    // Approve is off, and says when it turns on.
    expect(screen.getByTestId("letter-approve")).toBeDisabled();
    expect(screen.getByTestId("letter-approve-reason")).toBeVisible();
    // The aside.
    expect(screen.getByTestId("letter-banner-flagged")).toHaveTextContent("3 things need you");
    expect(screen.getByTestId("letter-glance")).toHaveTextContent(
      "62 words · 1 page · matches CV v1",
    );
    expect(screen.getByTestId("letter-glance")).toHaveTextContent(
      "1 fact from the company’s site: Builds ledgers for small shops",
    );
    expect(screen.getByTestId("letter-drafts-today")).toHaveTextContent(
      "Letter drafts today: 0 of 20",
    );
    expect(screen.getByTestId("letter-spend")).toHaveTextContent(
      "This draft costs about €0.002 · today’s AI: €0.04 of €0.30",
    );
    expect(screen.getByTestId("month-spend")).toBeVisible();
  });

  it("fixes a sentence by saying it's true, another by editing it, and sends what was typed", async () => {
    let said: unknown;
    let edited: unknown;
    renderLetter({
      [LETTER]: { body: letter({ draft: letterDraft() }) },
      [`POST ${DRAFTS}/1/sentences/s4/confirm`]: async (request) => {
        said = await request.json();
        return { body: letter({ draft: letterDraft() }) };
      },
      [`PATCH ${DRAFTS}/1/sentences/s6`]: async (request) => {
        edited = await request.json();
        return { body: letter({ draft: letterDraft() }) };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("letter-confirm-s4"));
    expect(screen.getByTestId("letter-confirm-input-s4")).toHaveAccessibleDescription(
      "This saves your words to your profile, then checks the sentence again.",
    );
    await user.type(
      screen.getByTestId("letter-confirm-input-s4"),
      "Known as the go-to person for billing",
    );
    await user.click(screen.getByTestId("letter-confirm-save-s4"));
    await waitFor(() => {
      expect(said).toEqual({ text: "Known as the go-to person for billing" });
    });
    await user.click(await screen.findByTestId("letter-edit-s6"));
    const input = screen.getByTestId("letter-edit-input-s6");
    await user.clear(input);
    await user.type(input, "Example Co builds ledgers for small shops.");
    await user.click(screen.getByTestId("letter-edit-save-s6"));
    await waitFor(() => {
      expect(edited).toEqual({ text: "Example Co builds ledgers for small shops." });
    });
  });

  it("removes a sentence, and answers the question the profile can't", async () => {
    let removed: unknown;
    let answered: unknown;
    renderLetter({
      [LETTER]: { body: letter({ draft: letterDraft() }) },
      [`PATCH ${DRAFTS}/1/sentences/s4`]: async (request) => {
        removed = await request.json();
        return { body: letter({ draft: letterDraft() }) };
      },
      [`PATCH ${DRAFTS}/1/answers/q11`]: async (request) => {
        answered = await request.json();
        return { body: fixed() };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("letter-remove-s4"));
    await waitFor(() => {
      expect(removed).toEqual({ remove: true });
    });
    await user.type(screen.getByTestId("letter-answer-input-q11"), "One month.");
    await user.click(screen.getByTestId("letter-answer-save-q11"));
    await waitFor(() => {
      expect(answered).toEqual({ text: "One month." });
    });
    // With nothing open, Approve is on.
    expect(await screen.findByTestId("letter-banner-clean")).toHaveTextContent(
      "Every line traces back",
    );
    expect(screen.getByTestId("letter-approve")).toBeEnabled();
    expect(screen.getByTestId("letter-answer-own-q11")).toHaveTextContent("Your answer");
  });

  it("copies an answer to the clipboard", async () => {
    renderLetter({ [LETTER]: { body: fixed() } });
    const user = userEvent.setup();
    const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    await user.click(await screen.findByTestId("letter-answer-copy-q10"));
    expect(writeText).toHaveBeenCalledWith(
      "Because you build ledgers for small shops, which is what I have built in Rust.",
    );
    expect(await screen.findByTestId("letter-answer-copy-q10")).toHaveTextContent("Copied");
  });

  it("approves, and then offers the PDF next to the CV", async () => {
    const approved = letter({
      draft: {
        ...letterDraft({ flagged: 0, can_approve: false }),
        status: "approved",
        version: 1,
        approved_at: "2026-11-01T10:00:00Z",
      },
      versions: [
        {
          n: 1,
          version: 1,
          language: "en",
          approved_at: "2026-11-01T10:00:00Z",
          pages: 1,
          size_bytes: 31_000,
          created_at: "2026-11-01T10:00:00Z",
          current: true,
          checks: {
            sections: { expected: [], missing: [] },
            keywords: { total: 0, found: 0, missing: [] },
            lines: { total: 5, missing: [] },
            text_layer: true,
            contact: { name: true, email: true, links: true },
            size: { pages: 1, length: 1, bytes: 31_000 },
          },
        },
      ],
    });
    const { onPipeline } = renderLetter({
      [LETTER]: { body: fixed() },
      [`POST ${DRAFTS}/1/approve`]: { body: approved },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("letter-approve"));
    expect(await screen.findByTestId("letter-banner-approved")).toHaveTextContent("Approved · v1");
    expect(screen.getByTestId("letter-download-pdf")).toHaveAttribute(
      "href",
      `/api/v1/opportunities/${ID}/letter/versions/1/pdf?download=1`,
    );
    // Frozen: no Approve and no Edit.
    expect(screen.queryByTestId("letter-approve")).toBeNull();
    await user.click(screen.getByTestId("letter-open-pdf"));
    expect(await screen.findByTestId("pdf-viewer")).toBeVisible();
    await user.click(screen.getByTestId("pdf-viewer-close"));
    await user.click(screen.getByTestId("letter-back-to-pipeline"));
    expect(onPipeline).toHaveBeenCalled();
  });

  it("says why it was refused, in plain words", async () => {
    renderLetter({
      [LETTER]: { body: fixed() },
      [`POST ${DRAFTS}/1/approve`]: {
        status: 409,
        body: { type: "about:blank", title: "x", status: 409, code: "letter_doesnt_fit" },
      },
    });
    await userEvent.setup().click(await screen.findByTestId("letter-approve"));
    expect(await screen.findByTestId("letter-error")).toHaveTextContent(
      "Doesn’t fit on 1 page: remove a few sentences or write a shorter letter.",
    );
  });

  it("makes the letter again with the length and language chosen", async () => {
    let sent: unknown;
    renderLetter({
      [LETTER]: { body: letter({ draft: letterDraft() }) },
      [`POST ${DRAFTS}`]: async (request) => {
        sent = await request.json();
        return { body: letter({ draft: letterDraft({ n: 2, size: "standard" }) }) };
      },
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("letter-size-standard"));
    await user.click(screen.getByTestId("letter-make-again"));
    await waitFor(() => {
      expect(sent).toEqual({ language: "en", size: "standard" });
    });
    expect(await screen.findByTestId("letter-draft-number")).toHaveTextContent("Draft 2");
  });
});

describe("Write the cover letter", () => {
  it("is off until the CV is approved, and says so", async () => {
    renderLetter({
      [LETTER]: {
        body: letter({ can_write: { allowed: false, reason: "no_approved_cv" } }),
      },
    });
    expect(await screen.findByTestId("letter-empty")).toBeVisible();
    expect(screen.getByTestId("letter-write")).toBeDisabled();
    expect(screen.getByTestId("letter-write-reason")).toHaveTextContent(
      "Approve your CV for this job first",
    );
  });

  it("writes it with one click and shows the review", async () => {
    renderLetter({
      [LETTER]: { body: letter() },
      [`POST ${DRAFTS}`]: { body: letter({ draft: letterDraft() }) },
    });
    const write = await screen.findByTestId("letter-write");
    expect(write).toHaveTextContent("Write the cover letter · about €0.002");
    await userEvent.setup().click(write);
    expect(await screen.findByTestId("letter-paper")).toBeVisible();
    expect(
      within(screen.getByTestId("letter-toolbar")).getByTestId("letter-draft-number"),
    ).toHaveTextContent("Draft 1");
  });

  it("explains a limit that was hit", async () => {
    renderLetter({
      [LETTER]: { body: letter() },
      [`POST ${DRAFTS}`]: {
        status: 429,
        body: {
          type: "about:blank",
          title: "x",
          status: 429,
          code: "letter_drafts_daily_cap",
        },
      },
    });
    await userEvent.setup().click(await screen.findByTestId("letter-write"));
    expect(await screen.findByTestId("letter-write-failed")).toHaveTextContent(
      "You’ve written 20 letter drafts today.",
    );
  });

  it("says the letter wasn't found", async () => {
    renderLetter({
      [LETTER]: {
        status: 404,
        body: { type: "about:blank", title: "x", status: 404, code: "not_found" },
      },
    });
    expect(await screen.findByTestId("letter-load-error")).toHaveTextContent(
      "This cover letter wasn’t found.",
    );
  });
});
