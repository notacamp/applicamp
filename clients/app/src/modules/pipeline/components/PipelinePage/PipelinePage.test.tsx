import { screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { UndoProvider } from "~/lib/ui";

import { opportunity, summary } from "../../../../../test/pipeline";
import type { Opportunity } from "../../api/opportunities";
import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";

import { PipelinePage } from "./PipelinePage";

const SECOND = "0190a1b2-0000-7000-8000-000000000002";

/** A server holding two opportunities, keeping what the page changes. */
function fakePipeline(extra: Record<string, Stub> = {}) {
  const store = new Map<string, Opportunity>([
    [opportunity().id, opportunity()],
    [
      SECOND,
      opportunity({
        id: SECOND,
        title: "Founding Engineer",
        company: "Sample Labs",
        stage: "shortlisted",
        new: false,
        source: "lever",
        match: null,
      }),
    ],
  ]);
  const patches: Record<string, unknown>[] = [];
  const requests = stubApi({
    "GET /api/v1/opportunities": (request) => {
      const query = new URL(request.url).searchParams;
      const stage = query.get("stage");
      const rows = [...store.values()].filter(
        (row) => !row.dismissed && (stage === null || row.stage === stage),
      );
      return { body: { opportunities: rows.map((row) => summary(row)) } };
    },
    [`GET /api/v1/opportunities/${opportunity().id}`]: () => ({
      body: store.get(opportunity().id),
    }),
    [`GET /api/v1/opportunities/${SECOND}`]: () => ({ body: store.get(SECOND) }),
    [`PATCH /api/v1/opportunities/${opportunity().id}`]: async (request) => {
      const changes = (await request.json()) as Record<string, unknown>;
      patches.push(changes);
      const current = store.get(opportunity().id) ?? opportunity();
      const kept = Object.fromEntries(Object.entries(changes).filter(([key]) => key !== "opened"));
      const next = {
        ...current,
        ...kept,
        ...(changes["opened"] === true ? { new: false } : {}),
      };
      store.set(opportunity().id, next);
      return { body: next };
    },
    ...extra,
  });
  return { patches, requests, store };
}

function renderPage(options: { newOnly?: boolean; adding?: boolean } = {}) {
  const onNewOnly = vi.fn<(newOnly: boolean) => void>();
  const onAdding = vi.fn<(open: boolean) => void>();
  function Harness() {
    const [adding, setAdding] = useState(options.adding ?? false);
    return (
      <PipelinePage
        newOnly={options.newOnly ?? false}
        onNewOnly={onNewOnly}
        adding={adding}
        onAdding={(open) => {
          onAdding(open);
          setAdding(open);
        }}
        search={<button type="button">Search now</button>}
      />
    );
  }
  renderWithProviders(
    <UndoProvider>
      <Harness />
    </UndoProvider>,
  );
  return { onNewOnly, onAdding };
}

describe("the pipeline page", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("lists every opportunity in a table and opens one in the peek, marking it seen", async () => {
    const { patches } = fakePipeline();
    const user = userEvent.setup();
    renderPage();

    const table = await screen.findByTestId("pipeline-table");
    expect(table).toHaveTextContent("Staff Engineer");
    expect(table).toHaveTextContent("Example Co");
    expect(table).toHaveTextContent("Match 82");
    expect(table).toHaveTextContent("Found");
    expect(table).toHaveTextContent("Greenhouse");
    expect(table).toHaveTextContent("Founding Engineer");
    expect(table).toHaveTextContent("Shortlisted");

    const [first] = screen.getAllByTestId("pipeline-row-open");
    if (first === undefined) {
      throw new TypeError("a row");
    }
    await user.click(first);
    const peek = await screen.findByTestId("peek");
    expect(await screen.findByTestId("peek-why")).toHaveTextContent("Your roles name this one");
    expect(screen.getByTestId("peek-gaps")).toHaveTextContent("On-call duty");
    expect(screen.getByTestId("peek-description")).toHaveTextContent("invented software");
    expect(screen.getByTestId("peek-where")).toHaveTextContent("Remote - Europe · Remote");
    expect(peek).toHaveTextContent("€90–110k");
    expect(screen.getByTestId("peek-open-posting")).toHaveAttribute(
      "href",
      "https://example.com/jobs/1",
    );
    expect(screen.getByTestId("peek-open-posting")).toHaveAttribute("rel", "noopener noreferrer");
    await waitFor(() => {
      expect(patches).toContainEqual({ opened: true });
    });
  });

  it("moves an opportunity to Shortlisted from the peek", async () => {
    const { patches } = fakePipeline();
    const user = userEvent.setup();
    renderPage();
    const [first] = await screen.findAllByTestId("pipeline-row-open");
    if (first === undefined) {
      throw new TypeError("a row");
    }
    await user.click(first);
    const stage = await screen.findByTestId("peek-stage");
    await user.selectOptions(stage, "shortlisted");
    await waitFor(() => {
      expect(patches).toContainEqual({ stage: "shortlisted" });
    });
    await user.click(screen.getByTestId("peek-close"));
    expect(screen.queryByTestId("peek")).toBeNull();
  });

  it("keeps notes and the next step", async () => {
    const { patches } = fakePipeline();
    const user = userEvent.setup();
    renderPage();
    const [first] = await screen.findAllByTestId("pipeline-row-open");
    if (first === undefined) {
      throw new TypeError("a row");
    }
    await user.click(first);
    await user.click(await screen.findByTestId("peek-next-step"));
    await user.keyboard("Tailor the CV{Enter}");
    await waitFor(() => {
      expect(patches).toContainEqual({ next_step: "Tailor the CV" });
    });
    await user.click(screen.getByTestId("peek-notes-edit"));
    await user.keyboard("Ask about the team");
    await user.click(screen.getByTestId("peek-notes-edit-save"));
    await waitFor(() => {
      expect(patches).toContainEqual({ notes: "Ask about the team" });
    });
  });

  it("says Not for me, with an undo", async () => {
    const { patches } = fakePipeline();
    const user = userEvent.setup();
    renderPage();
    const [first] = await screen.findAllByTestId("pipeline-row-open");
    if (first === undefined) {
      throw new TypeError("a row");
    }
    await user.click(first);
    await user.click(await screen.findByTestId("peek-not-for-me"));
    await waitFor(() => {
      expect(screen.queryByTestId("peek")).toBeNull();
    });
    expect(patches).toContainEqual({ dismissed: true });
    expect(document.body).toHaveTextContent("Not for me · Staff Engineer");
    await waitFor(() => {
      expect(screen.getByTestId("pipeline-table")).not.toHaveTextContent("Staff Engineer");
    });
  });

  it("shows the board by stage, and moves through it with J, K and Enter", async () => {
    fakePipeline();
    const user = userEvent.setup();
    renderPage();
    await screen.findByTestId("pipeline-table");
    await user.click(screen.getByTestId("pipeline-view-board"));
    expect(screen.getByTestId("pipeline-column-found")).toHaveTextContent("Staff Engineer");
    expect(screen.getByTestId("pipeline-column-shortlisted")).toHaveTextContent(
      "Founding Engineer",
    );
    expect(screen.getByTestId("pipeline-column-offer")).toHaveTextContent("Nothing here");

    await user.keyboard("j");
    await user.keyboard("j");
    await user.keyboard("k");
    await user.keyboard("{Enter}");
    await waitFor(() => {
      expect(screen.getByTestId("peek")).toHaveAttribute("aria-label", "Staff Engineer");
    });
  });

  it("filters by stage", async () => {
    const { requests } = fakePipeline();
    const user = userEvent.setup();
    renderPage();
    await screen.findByTestId("pipeline-table");
    await user.selectOptions(screen.getByTestId("pipeline-stage-filter"), "offer");
    expect(await screen.findByTestId("pipeline-no-results")).toHaveTextContent("Nothing here");
    expect(requests.some(({ route }) => route === "GET /api/v1/opportunities")).toBe(true);
  });

  it("says when there is nothing yet", async () => {
    stubApi({ "GET /api/v1/opportunities": { body: { opportunities: [] } } });
    renderPage();
    expect(await screen.findByTestId("pipeline-empty")).toHaveTextContent("No opportunities yet");
  });

  it("says when it cannot load, with a way to try again", async () => {
    let failing = true;
    stubApi({
      "GET /api/v1/opportunities": () =>
        failing
          ? { status: 500, body: { code: "internal_error" } }
          : { body: { opportunities: [] } },
    });
    const user = userEvent.setup();
    renderPage({ newOnly: true });
    const error = await screen.findByTestId("pipeline-load-error");
    expect(error).toHaveTextContent("The pipeline couldn’t be loaded.");
    failing = false;
    await user.click(error.querySelector("button") ?? error);
    expect(await screen.findByTestId("pipeline-no-results")).toBeInTheDocument();
  });

  it("shows the new matches only, until asked for everything", async () => {
    fakePipeline();
    const user = userEvent.setup();
    const { onNewOnly } = renderPage({ newOnly: true });
    await user.click(await screen.findByTestId("pipeline-show-all"));
    expect(onNewOnly).toHaveBeenCalledWith(false);
  });

  it("shows cards on a phone", async () => {
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches: true,
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }));
    fakePipeline();
    renderPage();
    expect(await screen.findByTestId("pipeline-cards")).toHaveTextContent("Staff Engineer");
    expect(screen.getAllByTestId("pipeline-row")).toHaveLength(2);
    expect(screen.queryByTestId("pipeline-table")).toBeNull();
  });

  it("adds a job by its link and opens it to type its role", async () => {
    const added = opportunity({
      id: "0190a1b2-0000-7000-8000-000000000003",
      title: "",
      company: "",
      location: null,
      work_mode: null,
      source: "link",
      url: "https://jobs.example.com/42",
      match: null,
      why: [],
      gaps: [],
      description: null,
      salary_text: null,
      posted_at: null,
      new: false,
      editable: true,
    });
    fakePipeline({
      "POST /api/v1/opportunities": async (request) => {
        const body = (await request.json()) as { url: string };
        return body.url === "https://jobs.example.com/42"
          ? { status: 201, body: added }
          : { status: 400, body: { code: "invalid_url" } };
      },
      [`GET /api/v1/opportunities/${added.id}`]: { body: added },
      [`PATCH /api/v1/opportunities/${added.id}`]: async (request) => ({
        body: { ...added, ...((await request.json()) as Record<string, unknown>) },
      }),
    });
    const { onAdding } = renderPage({ adding: true });
    const user = userEvent.setup();

    const url = await screen.findByTestId("new-opportunity-url");
    await user.type(url, "not a link");
    await user.click(screen.getByTestId("new-opportunity-submit"));
    expect(document.body).toHaveTextContent("Paste a link that starts with https://.");

    await user.clear(url);
    await user.type(url, "https://jobs.example.com/42");
    await user.click(screen.getByTestId("new-opportunity-submit"));
    await waitFor(() => {
      expect(onAdding).toHaveBeenCalledWith(false);
    });
    await waitFor(() => {
      expect(screen.getByTestId("peek")).toHaveAttribute("aria-label", "Untitled role");
    });
    expect(screen.getByTestId("peek-title")).toHaveTextContent("Type the role");
    await user.click(screen.getByTestId("peek-title"));
    await user.keyboard("Data Engineer{Enter}");
    await user.click(screen.getByTestId("peek-company-edit"));
    await user.keyboard("Invented Ltd{Enter}");
  });

  it("opens New from its button", async () => {
    fakePipeline();
    const user = userEvent.setup();
    const { onAdding } = renderPage();
    await user.click(await screen.findByTestId("pipeline-new"));
    expect(onAdding).toHaveBeenCalledWith(true);
  });
});
