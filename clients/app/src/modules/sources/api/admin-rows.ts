import { withFinder, type AdminCandidate, type AdminOverview, type BoardNeed } from "./admin";
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
      tags: withFinder(board).tags ?? [],
    };
  });
}

export interface AdminFilters {
  search: string;
  kind: RowKind | "all";
  status: RowStatus | "all";
  tag: string | "all";
}

export const NO_FILTERS: AdminFilters = { search: "", kind: "all", status: "all", tag: "all" };

/** The rows that match the search and every filter. */
export function filterRows(rows: readonly AdminRow[], filters: AdminFilters): AdminRow[] {
  const needle = filters.search.trim().toLowerCase();
  return rows.filter((row) => {
    if (filters.kind !== "all" && row.kind !== filters.kind) {
      return false;
    }
    if (filters.status !== "all" && row.status !== filters.status) {
      return false;
    }
    if (filters.tag !== "all" && !row.tags.includes(filters.tag)) {
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

/** The kinds of jobs boards are tagged with, most common first, at most `limit`. */
export function topTags(rows: readonly AdminRow[], limit = 6): string[] {
  const counts = new Map<string, number>();
  for (const row of rows) {
    for (const tag of row.tags) {
      counts.set(tag, (counts.get(tag) ?? 0) + 1);
    }
  }
  return [...counts.entries()]
    .toSorted((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, limit)
    .map(([tag]) => tag);
}
