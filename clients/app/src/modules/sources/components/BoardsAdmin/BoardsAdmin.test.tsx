import { screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import {
  adminBoard,
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
import type { AdminBoard } from "../../api/admin";

import { BoardsAdmin } from "./BoardsAdmin";

function server() {
  const lines = new Map<string, AdminBoard>([
    [HIMALAYAS, adminBoard()],
    [
      WAITING,
      adminBoard({
        id: WAITING,
        name: "Example Remote",
        status: "candidate",
        stage: "waiting",
        need: "permission",
        detail: "You may not scrape this site with automated tools.",
        tags: ["remote"],
      }),
    ],
    [
      declined().id,
      adminBoard({
        id: declined().id,
        name: "Landing.jobs",
        status: "declined",
        stage: "not_allowed",
      }),
    ],
  ]);
  const posted: { route: string; body: unknown }[] = [];
  const asked: { at: string | null } = { at: null };
  const requests = stubApi({
    "GET /api/v1/boards/admin": () => ({ body: adminOverview() }),
    "GET /api/v1/boards/admin/list": (request) => {
      const query = new URL(request.url).searchParams;
      const show = query.get("show");
      const q = (query.get("q") ?? "").toLowerCase();
      const boards = [...lines.values()].filter((line) => {
        const stage = { notAllowed: "not_allowed" }[show ?? ""] ?? show;
        if (show === "company" && line.group !== "company") {
          return false;
        }
        if (stage !== null && show !== "company" && show !== "site" && line.stage !== stage) {
          return false;
        }
        return q === "" || `${line.name} ${line.tags.join(" ")}`.toLowerCase().includes(q);
      });
      return { body: { boards, next_cursor: null } };
    },
    [`GET /api/v1/boards/${WAITING}`]: () => ({
      body:
        lines.get(WAITING)?.status === "approved"
          ? detail(board({ id: WAITING, name: "Example Remote" }))
          : detail(waiting(), {
              blockers: [{ code: "terms_forbid", detail: "You may not scrape this site." }],
            }),
    }),
    [`GET /api/v1/boards/${HIMALAYAS}`]: () => ({ body: detail(board()) }),
    [`GET /api/v1/boards/${WAITING}/permission-email`]: () => ({
      body: permissionEmail({ asked_at: asked.at }),
    }),
    [`POST /api/v1/boards/${WAITING}/permission/asked`]: () => {
      asked.at = "2026-11-04T10:00:00Z";
      return { body: permissionEmail({ asked_at: asked.at }) };
    },
    [`POST /api/v1/boards/${WAITING}/permission/answer`]: async (request) => {
      posted.push({
        route: new URL(request.url).pathname,
        body: JSON.parse(await request.text()) as unknown,
      });
      const line = lines.get(WAITING);
      if (line !== undefined) {
        lines.set(WAITING, { ...line, status: "approved", stage: "reading", need: null });
      }
      return { body: detail(board()) };
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
    expect(all).toHaveTextContent("All sites");
    expect(await within(all).findAllByTestId("boards-admin-row")).toHaveLength(3);
    expect(await screen.findByTestId("boards-admin-status-notAllowed")).toHaveTextContent(
      "Not allowed",
    );
  });

  it("asks the server to filter and search, and says when nothing matches", async () => {
    const { requests } = server();
    renderWithProviders(<BoardsAdmin back={null} />);
    const user = userEvent.setup();
    await screen.findByTestId("boards-admin-table");
    await user.selectOptions(screen.getByTestId("boards-admin-show"), "reading");
    await waitFor(() => {
      expect(screen.getAllByTestId("boards-admin-row")).toHaveLength(1);
    });
    expect(screen.getByTestId("boards-admin-table")).toHaveTextContent("Himalayas");
    expect(requests.some((r) => r.route === "GET /api/v1/boards/admin/list")).toBe(true);
    await user.selectOptions(screen.getByTestId("boards-admin-show"), "all");
    await user.type(screen.getByTestId("boards-admin-search"), "landing");
    await waitFor(() => {
      expect(screen.getAllByTestId("boards-admin-row")).toHaveLength(1);
    });
    expect(screen.getByTestId("boards-admin-table")).toHaveTextContent("Landing.jobs");
    await user.clear(screen.getByTestId("boards-admin-search"));
    await user.selectOptions(screen.getByTestId("boards-admin-show"), "company");
    expect(await screen.findByTestId("boards-admin-all-empty")).toHaveTextContent(
      "No site matches these filters.",
    );
    await user.click(screen.getByTestId("boards-admin-clear"));
    await waitFor(() => {
      expect(screen.getAllByTestId("boards-admin-row")).toHaveLength(3);
    });
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
      const field = permission.querySelector("input, textarea");
      expect(field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement).toBe(true);
      expect((field as HTMLInputElement).value).toContain(
        "Subject: May Applicamp show jobs from Example Remote?",
      );
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
    const [first] = await within(all).findAllByTestId("boards-admin-open");
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
