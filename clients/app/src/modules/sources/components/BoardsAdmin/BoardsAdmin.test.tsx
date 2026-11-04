import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import {
  adminOverview,
  board,
  declined,
  detail,
  HIMALAYAS,
  permissionEmail,
  waiting,
  WAITING,
} from "../../../../../test/boards";
import { renderWithProviders, stubApi } from "../../../../../test/render";
import type { BoardSummary } from "../../api/boards";

import { BoardsAdmin } from "./BoardsAdmin";

function server() {
  const boards = new Map<string, BoardSummary>([
    [HIMALAYAS, board()],
    [WAITING, waiting()],
    [declined().id, declined()],
  ]);
  const posted: { route: string; body: unknown }[] = [];
  let askedAt: string | null = null;
  const requests = stubApi({
    "GET /api/v1/boards": () => ({ body: { can_manage: true, boards: [...boards.values()] } }),
    "GET /api/v1/boards/admin": () => ({
      body: adminOverview({
        candidates:
          boards.get(WAITING)?.status === "candidate"
            ? adminOverview().candidates.map((candidate) => ({ ...candidate, asked_at: askedAt }))
            : [],
      }),
    }),
    [`GET /api/v1/boards/${WAITING}`]: () => ({
      body: detail(boards.get(WAITING) ?? waiting(), {
        blockers: [{ code: "terms_forbid", detail: "You may not scrape this site." }],
      }),
    }),
    [`GET /api/v1/boards/${HIMALAYAS}`]: () => ({ body: detail(board()) }),
    [`GET /api/v1/boards/${WAITING}/permission-email`]: () => ({
      body: permissionEmail({ asked_at: askedAt }),
    }),
    [`POST /api/v1/boards/${WAITING}/permission/asked`]: () => {
      askedAt = "2026-11-04T10:00:00Z";
      return { body: permissionEmail({ asked_at: askedAt }) };
    },
    [`POST /api/v1/boards/${WAITING}/permission/answer`]: async (request) => {
      posted.push({
        route: new URL(request.url).pathname,
        body: JSON.parse(await request.text()) as unknown,
      });
      boards.set(WAITING, { ...(boards.get(WAITING) ?? waiting()), status: "approved" });
      return { body: detail(boards.get(WAITING) ?? waiting()) };
    },
  });
  return { posted, requests };
}

describe("the job boards admin page", () => {
  it("shows the totals, this week, what waits for you and every board", async () => {
    server();
    renderWithProviders(<BoardsAdmin back={<span>back</span>} />);
    expect(await screen.findByTestId("boards-admin-totals")).toBeInTheDocument();
    expect(screen.getByTestId("boards-admin-total-reading")).toHaveTextContent("3,148");
    expect(screen.getByTestId("boards-admin-totals")).toHaveTextContent(
      "412 job sites and 2,736 companies",
    );
    expect(screen.getByTestId("boards-admin-total-waiting")).toHaveTextContent("1");
    const week = screen.getByTestId("boards-admin-week");
    expect(week).toHaveTextContent("Found 184 new sites and checked 190.");
    expect(week).toHaveTextContent("162 sites started reading.");
    const waitingList = screen.getByTestId("boards-admin-waiting");
    expect(waitingList).toHaveTextContent("Example Remote");
    expect(waitingList).toHaveTextContent("written permission");
    expect(within(waitingList).getByTestId("boards-admin-status-waiting")).toHaveTextContent(
      "Waiting for you",
    );
    const all = screen.getByTestId("boards-admin-all");
    expect(all).toHaveTextContent("All sites · 3");
    expect(within(all).getAllByTestId("boards-admin-row")).toHaveLength(3);
    expect(screen.getByTestId("boards-admin-status-notAllowed")).toHaveTextContent("Not allowed");
  });

  it("filters the table by status and by what is typed, and says when nothing matches", async () => {
    server();
    renderWithProviders(<BoardsAdmin back={null} />);
    const user = userEvent.setup();
    await screen.findByTestId("boards-admin-table");
    await user.selectOptions(screen.getByTestId("boards-admin-show"), "reading");
    expect(screen.getAllByTestId("boards-admin-row")).toHaveLength(1);
    expect(screen.getByTestId("boards-admin-table")).toHaveTextContent("Himalayas");
    await user.selectOptions(screen.getByTestId("boards-admin-show"), "all");
    await user.type(screen.getByTestId("boards-admin-search"), "landing");
    expect(screen.getAllByTestId("boards-admin-row")).toHaveLength(1);
    await user.clear(screen.getByTestId("boards-admin-search"));
    await user.selectOptions(screen.getByTestId("boards-admin-show"), "company");
    expect(screen.getByTestId("boards-admin-all-empty")).toHaveTextContent(
      "No board matches these filters.",
    );
    await user.click(screen.getByTestId("boards-admin-clear"));
    expect(screen.getAllByTestId("boards-admin-row")).toHaveLength(3);
  });

  it("puts the decision first: copy the email, say it was sent, then record their yes", async () => {
    const { posted } = server();
    renderWithProviders(<BoardsAdmin back={null} />);
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("boards-admin-waiting-open"));
    const peek = await screen.findByTestId("boards-admin-peek");
    const permission = await screen.findByTestId("boards-admin-permission");
    // The decision comes before the facts and the checks.
    expect(
      permission.compareDocumentPosition(screen.getByTestId("boards-admin-facts")) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await waitFor(() => {
      expect(permission).toHaveTextContent("May Applicamp show jobs from Example Remote?");
    });
    expect(permission).toHaveTextContent("Send it to the contact on its site.");
    expect(screen.getByTestId("boards-admin-checks")).toHaveTextContent(
      "You may not scrape this site.",
    );
    expect(screen.queryByTestId("boards-admin-yes")).toBeNull();
    await user.click(screen.getByTestId("boards-admin-asked"));
    expect(await screen.findByTestId("boards-admin-asked-on")).toHaveTextContent("Asked on");
    await user.click(screen.getByTestId("boards-admin-yes"));
    await waitFor(() => {
      expect(posted).toEqual([
        {
          route: `/api/v1/boards/${WAITING}/permission/answer`,
          body: { answer: "yes", kind: "permission" },
        },
      ]);
    });
    // Approved: the panel now offers Pause, Check again and Stop reading it.
    expect(await within(peek).findByTestId("boards-admin-reading")).toBeInTheDocument();
  });

  it("asks before it stops reading a board", async () => {
    server();
    renderWithProviders(<BoardsAdmin back={null} />);
    const user = userEvent.setup();
    const all = await screen.findByTestId("boards-admin-all");
    const [first] = within(all).getAllByTestId("boards-admin-open");
    if (first === undefined) {
      throw new Error("no board in the table");
    }
    await user.click(first);
    await user.click(await screen.findByTestId("boards-admin-stop"));
    expect(screen.getByTestId("boards-admin-stop-confirm")).toHaveTextContent(
      "Stop reading Himalayas? Searches will no longer use it.",
    );
  });
});
