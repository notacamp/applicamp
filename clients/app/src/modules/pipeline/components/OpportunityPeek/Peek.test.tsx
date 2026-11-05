import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { UndoProvider } from "~/lib/ui";

import { cv, cvDraft, letter, letterDraft, opportunity } from "../../../../../test/pipeline";
import { renderWithProviders, stubApi } from "../../../../../test/render";

import { OpportunityPeek } from "./OpportunityPeek";

const ID = "0190a1b2-0000-7000-8000-000000000001";

const CHECKS = {
  sections: { expected: [], missing: [] },
  keywords: { total: 0, found: 0, missing: [] },
  lines: { total: 4, missing: [] },
  text_layer: true,
  contact: { name: true, email: true, links: true },
  size: { pages: 1, length: 1, bytes: 30_000 },
};

/** A CV with an approved version, so the cover letter line shows. */
function approvedCv() {
  return cv({
    draft: { ...cvDraft(), status: "approved", version: 1, flagged: 0, can_approve: false },
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
        checks: CHECKS,
      },
    ],
  });
}

function renderPeek(routes: Parameters<typeof stubApi>[0]) {
  stubApi({
    [`GET /api/v1/opportunities/${ID}`]: { body: opportunity({ id: ID, stage: "shortlisted" }) },
    [`GET /api/v1/opportunities/${ID}/cv`]: { body: approvedCv() },
    ...routes,
  });
  const handlers = { onPrepare: vi.fn(), onReview: vi.fn(), onLetter: vi.fn(), onClose: vi.fn() };
  renderWithProviders(
    <UndoProvider>
      <OpportunityPeek id={ID} {...handlers} />
    </UndoProvider>,
  );
  return handlers;
}

describe("The peek's cover letter line", () => {
  it("offers to write the letter once the CV is approved", async () => {
    const handlers = renderPeek({
      [`GET /api/v1/opportunities/${ID}/letter`]: { body: letter() },
    });
    expect(await screen.findByTestId("peek-letter-state")).toHaveTextContent(
      "No cover letter yet.",
    );
    await userEvent.setup().click(screen.getByTestId("peek-letter-review"));
    expect(handlers.onLetter).toHaveBeenCalledWith(ID);
    expect(screen.queryByTestId("peek-letter-view-pdf")).toBeNull();
  });

  it("says a draft needs you, with Review", async () => {
    const handlers = renderPeek({
      [`GET /api/v1/opportunities/${ID}/letter`]: { body: letter({ draft: letterDraft() }) },
    });
    expect(await screen.findByTestId("peek-letter-state")).toHaveTextContent(
      "Draft 1: 3 things need you.",
    );
    const review = screen.getByTestId("peek-letter-review");
    expect(review).toHaveTextContent("Review the letter");
    await userEvent.setup().click(review);
    expect(handlers.onLetter).toHaveBeenCalledWith(ID);
  });

  it("shows an approved letter with View PDF and Download", async () => {
    renderPeek({
      [`GET /api/v1/opportunities/${ID}/letter`]: {
        body: letter({
          draft: { ...letterDraft(), status: "approved", version: 2, flagged: 0 },
          versions: [
            {
              n: 2,
              version: 2,
              language: "en",
              approved_at: "2026-11-01T10:00:00Z",
              pages: 1,
              size_bytes: 31_000,
              created_at: "2026-11-01T10:00:00Z",
              current: true,
              checks: CHECKS,
            },
          ],
        }),
      },
    });
    expect(await screen.findByTestId("peek-letter-state")).toHaveTextContent("v2 approved.");
    expect(screen.getByTestId("peek-letter-download-pdf")).toHaveAttribute(
      "href",
      `/api/v1/opportunities/${ID}/letter/versions/2/pdf?download=1`,
    );
    await userEvent.setup().click(screen.getByTestId("peek-letter-view-pdf"));
    expect(await screen.findByTestId("pdf-viewer")).toBeVisible();
  });

  it("is not there before the CV is approved", async () => {
    renderPeek({
      [`GET /api/v1/opportunities/${ID}/cv`]: { body: cv() },
    });
    expect(await screen.findByTestId("peek-cv-state")).toBeVisible();
    expect(screen.queryByTestId("peek-letter")).toBeNull();
  });
});

describe("The peek's gaps", () => {
  const GAPS = [
    "Doesn't say if it hires in Portugal",
    "Salary not stated",
    "On-call duty is not on your profile",
  ];

  function renderGaps(locale: "en" | "pt-PT" | "fr") {
    stubApi({
      [`GET /api/v1/opportunities/${ID}`]: {
        body: opportunity({ id: ID, stage: "found", gaps: GAPS }),
      },
      [`GET /api/v1/opportunities/${ID}/cv`]: { status: 404 },
    });
    renderWithProviders(
      <UndoProvider>
        <OpportunityPeek id={ID} onPrepare={vi.fn()} onReview={vi.fn()} onClose={vi.fn()} />
      </UndoProvider>,
      { locale },
    );
  }

  it("say in the person's language what the app noted, and the model's as written", async () => {
    renderGaps("en");
    const gaps = await screen.findByTestId("peek-gaps");
    expect(gaps).toHaveTextContent("Doesn’t say if it hires in Portugal");
    expect(gaps).toHaveTextContent("Salary not stated");
    expect(gaps).toHaveTextContent("On-call duty is not on your profile");
  });

  it("name the country in the person's language", async () => {
    renderGaps("fr");
    const gaps = await screen.findByTestId("peek-gaps");
    expect(gaps).toHaveTextContent("Ne dit pas si elle recrute dans votre pays : Portugal");
    expect(gaps).toHaveTextContent("Salaire non indiqué");
  });
});
