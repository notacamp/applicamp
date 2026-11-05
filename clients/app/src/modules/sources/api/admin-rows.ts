import type { Schema } from "~/lib/api";

type AdminBoard = Schema<"AdminBoard">;
type BoardNeed = Schema<"BoardNeed">;

/** Where a board stands, as the page words it. */
export type RowStatus = "reading" | "waiting" | "checking" | "notAllowed" | "paused";
export type RowKind = "site" | "company";

/** One line of the table: the board with where it stands and what it waits for. */
export interface AdminRow {
  board: AdminBoard;
  status: RowStatus;
  kind: RowKind;
  need: BoardNeed | undefined;
  /** The first reason in the board's own words. */
  detail: string | undefined;
  askedAt: string | undefined;
  tags: readonly string[];
}

const STATUS = {
  reading: "reading",
  paused: "paused",
  waiting: "waiting",
  checking: "checking",
  not_allowed: "notAllowed",
} as const satisfies Record<AdminBoard["stage"], RowStatus>;

/** A line of the server's list as the page uses it. */
export function toRow(board: AdminBoard): AdminRow {
  return {
    board,
    status: STATUS[board.stage],
    kind: board.group === "company" ? "company" : "site",
    need: board.need ?? undefined,
    detail: board.detail ?? undefined,
    askedAt: board.asked_at ?? undefined,
    tags: board.tags,
  };
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
