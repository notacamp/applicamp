import { screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Schema } from "~/lib/api";

import { renderWithProviders, stubApi } from "../../../../../test/render";

import { CvsPage } from "./CvsPage";

const OPP = "0190a1b2-0000-7000-8000-000000000001";
const OTHER = "0190a1b2-0000-7000-8000-000000000002";

type Item = Schema<"CvListItem">;

function version(n: number, overrides: Partial<Item> = {}): Item {
  return {
    kind: "version",
    opportunity_id: OPP,
    title: "Staff Engineer",
    company: "Example Co",
    version: n,
    draft: 1,
    status: "approved",
    template: n === 1 ? "clean" : "compact",
    pages: 1,
    size_bytes: 30_000,
    language: "en",
    created_at: `2026-10-${n === 1 ? "30" : "31"}T09:00:00Z`,
    downloaded_at: n === 1 ? "2026-10-31T10:00:00Z" : null,
    current: n === 2,
    ...overrides,
  };
}

const draft: Item = {
  kind: "draft",
  opportunity_id: OTHER,
  title: "Backend Engineer",
  company: "Sample Labs",
  version: null,
  draft: 2,
  status: "draft",
  template: null,
  pages: null,
  size_bytes: null,
  language: "en",
  created_at: "2026-10-31T11:00:00Z",
  downloaded_at: null,
  current: false,
};

function render(items: Item[]) {
  stubApi({ "GET /api/v1/cvs": { body: { items } } });
  const handlers = { onReview: vi.fn(), onOpportunity: vi.fn(), onPipeline: vi.fn() };
  renderWithProviders(<CvsPage {...handlers} />);
  return handlers;
}

describe("The CVs page", () => {
  it("lists versions and drafts, filters them, and sends a draft to its review", async () => {
    const handlers = render([draft, version(2), version(1)]);
    expect(await screen.findAllByTestId("cvs-row")).toHaveLength(3);
    const rows = screen.getAllByTestId("cvs-row");
    const second = rows.at(1) ?? document.body;
    const third = rows.at(2) ?? document.body;
    expect(within(second).getByTestId("cvs-row-version")).toHaveTextContent("v2 · Current");
    expect(within(second).getByTestId("cvs-row-facts")).toHaveTextContent("Compact · 1 page");
    expect(within(third).getByTestId("cvs-download")).toHaveAttribute(
      "href",
      `/api/v1/opportunities/${OPP}/cv/versions/1/pdf?download=1`,
    );
    const user = userEvent.setup();
    await user.click(screen.getByTestId("cvs-filter-drafts"));
    expect(screen.getAllByTestId("cvs-row")).toHaveLength(1);
    await user.click(screen.getByTestId("cvs-review"));
    expect(handlers.onReview).toHaveBeenCalledWith(OTHER);
    await user.click(screen.getByTestId("cvs-opportunity"));
    expect(handlers.onOpportunity).toHaveBeenCalledWith(OTHER);
    await user.click(screen.getByTestId("cvs-filter-approved"));
    expect(screen.getAllByTestId("cvs-row")).toHaveLength(2);
  });

  it("says how to make the first one", async () => {
    const handlers = render([]);
    expect(await screen.findByTestId("cvs-empty")).toBeVisible();
    await userEvent.setup().click(screen.getByTestId("cvs-empty-pipeline"));
    expect(handlers.onPipeline).toHaveBeenCalled();
  });

  it("opens a PDF in the viewer, with Download and Close", async () => {
    render([version(1)]);
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("cvs-view"));
    expect(await screen.findByTestId("pdf-viewer")).toBeVisible();
    expect(screen.getByTestId("pdf-viewer-download")).toHaveAttribute(
      "href",
      `/api/v1/opportunities/${OPP}/cv/versions/1/pdf?download=1`,
    );
    await user.click(screen.getByTestId("pdf-viewer-close"));
    expect(screen.queryByTestId("pdf-viewer")).toBeNull();
  });
});
