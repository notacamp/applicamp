import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import {
  board,
  declined,
  detail,
  discoveryRun,
  HIMALAYAS,
  run,
  waiting,
  WAITING,
} from "../../../../../test/boards";
import { renderWithProviders, stubApi, type Stub } from "../../../../../test/render";
import type { BoardDetail, BoardSummary } from "../../api/boards";

import { BoardsPage, type BoardsTab } from "./BoardsPage";

/** A server with an approved board, one waiting and one declined, keeping the owner's decisions. */
function fakeBoards(canManage: boolean, extra: Record<string, Stub> = {}) {
  const boards = new Map<string, BoardSummary>([
    [HIMALAYAS, board()],
    [WAITING, waiting()],
    [declined().id, declined()],
  ]);
  const posted: { route: string; body: unknown }[] = [];
  const answer = (id: string): { body: BoardDetail } => {
    const summary = boards.get(id) ?? board();
    return { body: detail(summary, { can_manage: canManage }) };
  };
  const decide =
    (id: string, status: BoardSummary["status"]): Stub =>
    async (request) => {
      const text = await request.text();
      posted.push({
        route: new URL(request.url).pathname,
        body: text === "" ? null : JSON.parse(text),
      });
      const current = boards.get(id) ?? board();
      boards.set(id, { ...current, status });
      return answer(id);
    };
  const requests = stubApi({
    "GET /api/v1/boards": () => ({
      body: { can_manage: canManage, boards: [...boards.values()] },
    }),
    [`GET /api/v1/boards/${HIMALAYAS}`]: () => answer(HIMALAYAS),
    [`GET /api/v1/boards/${WAITING}`]: () => answer(WAITING),
    [`POST /api/v1/boards/${WAITING}/approve`]: decide(WAITING, "approved"),
    [`POST /api/v1/boards/${WAITING}/decline`]: decide(WAITING, "declined"),
    [`POST /api/v1/boards/${WAITING}/check`]: decide(WAITING, "candidate"),
    [`POST /api/v1/boards/${HIMALAYAS}/pause`]: decide(HIMALAYAS, "paused"),
    [`PUT /api/v1/boards/${WAITING}/config`]: decide(WAITING, "candidate"),
    "GET /api/v1/boards/scans": (request) => {
      const query = new URL(request.url).searchParams;
      if (query.get("cursor") !== null) {
        return { body: { runs: [discoveryRun()], next_cursor: null } };
      }
      return { body: { runs: [run()], next_cursor: "2026-10-25T21:00:00Z" } };
    },
    ...extra,
  });
  return { boards, posted, requests };
}

function renderPage(initial: BoardsTab = "boards") {
  function Harness() {
    const [tab, setTab] = useState<BoardsTab>(initial);
    return <BoardsPage tab={tab} onTab={setTab} />;
  }
  return renderWithProviders(<Harness />);
}

describe("the boards page", () => {
  it("lists the boards by status, each with its salary line, and opens one with its check", async () => {
    fakeBoards(false);
    renderPage();
    const approved = await screen.findByTestId("boards-section-approved");
    expect(approved).toHaveTextContent("Himalayas");
    expect(approved).toHaveTextContent("from 64 of 210 jobs (30% state a salary)");
    expect(approved).toHaveTextContent("9 matched this week");
    const waitingSection = screen.getByTestId("boards-section-waiting");
    expect(waitingSection).toHaveTextContent("Example Remote");
    expect(waitingSection).toHaveTextContent("Few salaries");
    expect(waitingSection).toHaveTextContent("To check: 1");
    const declinedSection = screen.getByTestId("boards-section-declined");
    expect(declinedSection).toHaveTextContent("No salary data");
    expect(declinedSection).toHaveTextContent("its terms forbid scraping");

    const user = userEvent.setup();
    await user.click(within(waitingSection).getByTestId("boards-row"));
    const peek = await screen.findByTestId("board-peek");
    await waitFor(() => {
      expect(screen.getByTestId("board-robots")).toHaveTextContent("robots.txt allows its feed");
    });
    expect(screen.getByTestId("board-terms-sentences")).toHaveTextContent(
      "You may not scrape this site",
    );
    expect(screen.getByTestId("board-sample")).toHaveTextContent("Senior Backend Engineer");
    expect(screen.getByTestId("board-attribution")).toHaveTextContent("Example Remote");
    expect(screen.getByTestId("board-terms")).toHaveTextContent("At most 4 reads a day.");
    expect(screen.getByTestId("board-flags")).toHaveTextContent("scraping");
    expect(peek).not.toHaveTextContent("Your decision");
    expect(screen.queryByTestId("board-approve")).toBeNull();
    await user.click(screen.getByTestId("board-peek-close"));
    await waitFor(() => {
      expect(screen.queryByTestId("board-peek")).toBeNull();
    });
  });

  it("lets the owner approve, decline, check and edit a waiting board", async () => {
    const { posted } = fakeBoards(true);
    renderPage();
    const user = userEvent.setup();
    const waitingSection = await screen.findByTestId("boards-section-waiting");
    await user.click(within(waitingSection).getByTestId("boards-row"));
    await screen.findByTestId("board-manage");

    // A config that isn't JSON, or isn't an object, never leaves the page.
    const config = screen.getByTestId("board-config");
    await user.clear(config);
    await user.type(config, "not json");
    await user.click(screen.getByTestId("board-config-save"));
    expect(screen.getByTestId("board-manage")).toHaveTextContent("That isn’t valid JSON.");
    await user.clear(config);
    await user.type(config, "[[1]");
    await user.click(screen.getByTestId("board-config-save"));
    expect(screen.getByTestId("board-manage")).toHaveTextContent("The config is a JSON object.");
    await user.clear(config);
    await user.type(config, '{{"poll_minutes": 120}');
    await user.click(screen.getByTestId("board-config-save"));
    await waitFor(() => {
      expect(posted.map((p) => p.route)).toContain(`/api/v1/boards/${WAITING}/config`);
    });
    expect(posted.find((p) => p.route.endsWith("/config"))?.body).toEqual({
      config: { poll_minutes: 120 },
    });

    await user.click(screen.getByTestId("board-check"));
    await waitFor(() => {
      expect(posted.map((p) => p.route)).toContain(`/api/v1/boards/${WAITING}/check`);
    });

    // Decline needs a reason.
    expect(screen.getByTestId("board-decline")).toBeDisabled();
    await user.type(screen.getByTestId("board-decline-reason"), "Its jobs are stale.");
    await user.click(screen.getByTestId("board-decline"));
    await waitFor(() => {
      expect(posted.find((p) => p.route.endsWith("/decline"))?.body).toEqual({
        reason: "Its jobs are stale.",
      });
    });
  });

  it("approves a waiting board, and says why one can't be", async () => {
    const { posted } = fakeBoards(true, {
      [`GET /api/v1/boards/${WAITING}`]: () => ({
        body: detail(waiting(), { approvable: true, can_manage: true }),
      }),
    });
    renderPage();
    const user = userEvent.setup();
    const waitingSection = await screen.findByTestId("boards-section-waiting");
    await user.click(within(waitingSection).getByTestId("boards-row"));
    const approve = await screen.findByTestId("board-approve");
    await waitFor(() => {
      expect(approve).toBeEnabled();
    });
    await user.click(approve);
    await waitFor(() => {
      expect(posted.map((p) => p.route)).toContain(`/api/v1/boards/${WAITING}/approve`);
    });
  });

  it("shows why a board is blocked and what an action answered", async () => {
    fakeBoards(true, {
      [`GET /api/v1/boards/${WAITING}`]: () => ({
        body: detail(waiting(), {
          approvable: false,
          blocked: "robots_disallowed",
          can_manage: true,
        }),
      }),
      [`POST /api/v1/boards/${WAITING}/check`]: {
        status: 403,
        body: { type: "about:blank", title: "Forbidden", status: 403, code: "not_board_owner" },
      },
    });
    renderPage();
    const user = userEvent.setup();
    const waitingSection = await screen.findByTestId("boards-section-waiting");
    await user.click(within(waitingSection).getByTestId("boards-row"));
    expect(await screen.findByTestId("board-blocked")).toHaveTextContent(
      "Its robots rules forbid this feed.",
    );
    expect(screen.getByTestId("board-approve")).toBeDisabled();
    await user.click(screen.getByTestId("board-check"));
    expect(await screen.findByTestId("board-action-error")).toHaveTextContent(
      "Only the instance’s board owners can do that.",
    );
  });

  it("pauses an approved board", async () => {
    const { posted } = fakeBoards(true);
    renderPage();
    const user = userEvent.setup();
    const approved = await screen.findByTestId("boards-section-approved");
    await user.click(within(approved).getByTestId("boards-row"));
    await user.click(await screen.findByTestId("board-pause"));
    await waitFor(() => {
      expect(posted.map((p) => p.route)).toContain(`/api/v1/boards/${HIMALAYAS}/pause`);
    });
    expect(screen.getByTestId("board-totals")).toHaveTextContent("7 days: 412 read · 9 matched");
  });

  it("takes a suggestion, and says what is wrong with one", async () => {
    let calls = 0;
    fakeBoards(false, {
      "POST /api/v1/boards/suggestions": () => {
        calls += 1;
        return calls === 1
          ? {
              status: 400,
              body: { type: "about:blank", title: "Bad", status: 400, code: "invalid_url" },
            }
          : { status: 202, body: detail(waiting({ name: "jobs.example.org" })) };
      },
    });
    renderPage();
    const user = userEvent.setup();
    await screen.findByTestId("boards-section-approved");
    await user.click(screen.getByTestId("boards-suggest"));
    expect(calls).toBe(0);
    await user.type(screen.getByTestId("boards-suggest-url"), "http://jobs.example.org");
    await user.click(screen.getByTestId("boards-suggest"));
    await waitFor(() => {
      expect(screen.getByTestId("boards-suggest-form")).toHaveTextContent(
        "Paste the board’s address, starting with https://.",
      );
    });
    await user.clear(screen.getByTestId("boards-suggest-url"));
    await user.type(screen.getByTestId("boards-suggest-url"), "https://jobs.example.org");
    await user.click(screen.getByTestId("boards-suggest"));
    expect(await screen.findByTestId("boards-suggest-done")).toHaveTextContent(
      "jobs.example.org is waiting for you",
    );
  });

  it("says when the boards can't be loaded", async () => {
    stubApi({ "GET /api/v1/boards": { status: 500 } });
    renderPage();
    expect(await screen.findByTestId("boards-load-error")).toBeVisible();
  });

  it("shows what each search and discovery run scanned, older runs on demand", async () => {
    fakeBoards(false);
    renderPage("scanned");
    const runs = await screen.findAllByTestId("scanned-run");
    expect(runs[0]).toHaveTextContent("nightly search");
    const lines = within(runs[0] ?? document.body).getAllByTestId("scanned-line");
    expect(lines[0]).toHaveTextContent(
      "Himalayas: 412 read → 30 passed your requirements → 30 scored by AI → 9 matched",
    );
    expect(lines[1]).toHaveTextContent("Remotive: skipped (polling limit)");
    expect(lines[2]).toHaveTextContent("Jobicy: couldn’t be read (rate_limited)");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("scanned-more"));
    await waitFor(() => {
      expect(screen.getAllByTestId("scanned-run")).toHaveLength(2);
    });
    expect(screen.getAllByTestId("scanned-run")[1]).toHaveTextContent(
      "Example Remote: checked · Feed at /feed.xml",
    );
    expect(screen.queryByTestId("scanned-more")).toBeNull();
    await user.selectOptions(screen.getByTestId("scanned-board-filter"), HIMALAYAS);
    await waitFor(() => {
      expect(screen.getByTestId("scanned-board-filter")).toHaveValue(HIMALAYAS);
    });
    await user.click(screen.getByTestId("boards-tab-boards"));
    expect(await screen.findByTestId("boards-section-approved")).toBeVisible();
  });

  it("says when nothing was scanned yet", async () => {
    stubApi({
      "GET /api/v1/boards": { body: { can_manage: false, boards: [] } },
      "GET /api/v1/boards/scans": { body: { runs: [], next_cursor: null } },
    });
    renderPage("scanned");
    expect(await screen.findByTestId("scanned-empty")).toBeVisible();
  });
});
