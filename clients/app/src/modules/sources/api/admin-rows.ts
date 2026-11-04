import type { AdminCandidate, type AdminOverview, type BoardNeed } from "./admin";
import type { BoardSummary } from "./boards";

/** Where a board stands, as the page words it. */
export type RowStatus = "reading" | "waiting" | "checking" | "notAllowed" | "paused";
export type RowKind = "site" | "company";

/** One line of "All boards": the board with where it stands and what it waits for. */
export interface AdminRow {
  board: BoardSummary;
  status: RowStatus;
  kind: RowKind;
  need: BoardNeed | undefined;
  /** The first reason in the board's own words. */
  detail: string | undefined;
  askedAt: string | undefined;
  tags: readonly string[];
}

/** Every board with its status: approved and paused by their own, a candidate by the overview. */
export function adminRows(boards: readonly BoardSummary[], overview: AdminOverview): AdminRow[] {
  const candidates = new Map<string, AdminCandidate>(overview.candidates.map((c) => [c.id, c]));
  return boards.map((board) => {
    const candidate = candidates.get(board.id);
    let status: RowStatus = "checking";
    switch (board.status) {
      case "approved": {
        status = "reading";
        break;
      }
      case "paused": {
        status = "paused";
        break;
      }
      case "declined": {
        status = "notAllowed";
        break;
      }
      case "candidate": {
        if (candidate !== undefined) {
          status = candidate.stage === "not_allowed" ? "notAllowed" : candidate.stage;
        }
        break;
      }
    }
    return {
      board,
      status,
      kind: "site",
      need: candidate?.need ?? undefined,
      detail: candidate?.detail ?? undefined,
      askedAt: candidate?.asked_at ?? undefined,
      tags: board.tags,
    };
  });
}

/** What the "Show" select picks: every board, one status, or one kind of board. */
export type ShowFilter = RowStatus | RowKind | "all";

export const SHOW_OPTIONS: readonly ShowFilter[] = [
  "all",
  "waiting",
  "checking",
  "reading",
  "notAllowed",
  "paused",
  "site",
  "company",
];

export interface AdminFilters {
  search: string;
  show: ShowFilter;
}

export const NO_FILTERS: AdminFilters = { search: "", show: "all" };

/** The rows that match the search (name, site, place, kinds of jobs) and what is shown. */
export function filterRows(rows: readonly AdminRow[], filters: AdminFilters): AdminRow[] {
  const needle = filters.search.trim().toLowerCase();
  return rows.filter((row) => {
    if (filters.show !== "all" && row.status !== filters.show && row.kind !== filters.show) {
      return false;
    }
    if (needle === "") {
      return true;
    }
    const haystack = [row.board.name, row.board.homepage, row.board.country ?? "", ...row.tags]
      .join(" ")
      .toLowerCase();
    return haystack.includes(needle);
  });
}
