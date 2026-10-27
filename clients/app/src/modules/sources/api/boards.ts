import {
  queryOptions,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import type { TFunction } from "i18next";

import { api, CSRF_HEADERS, problemCode, unwrap, type Schema } from "~/lib/api";

import { formatEuros } from "./searches";

/** A job board as the Boards page lists it: its ranking, its status, the person's totals. */
export type BoardSummary = Schema<"BoardSummary">;
/** The boards, highest average salary first, and whether the person manages them. */
export type BoardList = Schema<"BoardList">;
/** A board, whole: its check, config and terms. */
export type BoardDetail = Schema<"BoardDetail">;
export type BoardStatus = Schema<"BoardStatus">;
export type BoardRanking = Schema<"BoardRanking">;
export type BoardReport = Schema<"BoardReport">;
/** A search or a discovery run, and what it scanned on each board. */
export type ScanRun = Schema<"ScanRun">;
export type ScanLine = Schema<"ScanLine">;
export type ScanPage = Schema<"ScanPage">;

export const BOARDS_QUERY_KEY = ["boards"] as const;
const LIST_KEY = [...BOARDS_QUERY_KEY, "list"] as const;
const detailKey = (id: string) => [...BOARDS_QUERY_KEY, "detail", id] as const;
const scansKey = (board: string | null) => [...BOARDS_QUERY_KEY, "scans", board] as const;

/** The boards (`GET /api/v1/boards`). */
export const boardsQuery = queryOptions({
  queryKey: LIST_KEY,
  queryFn: async ({ signal }): Promise<BoardList> =>
    unwrap(await api.GET("/api/v1/boards", { signal })),
});

export function useBoards() {
  return useQuery(boardsQuery);
}

/** One board (`GET /api/v1/boards/{id}`); none while `id` is null. */
export function useBoard(id: string | null) {
  return useQuery({
    queryKey: detailKey(id ?? ""),
    enabled: id !== null,
    queryFn: async ({ signal }): Promise<BoardDetail> =>
      unwrap(await api.GET("/api/v1/boards/{id}", { params: { path: { id: id ?? "" } }, signal })),
  });
}

/** What was scanned, newest first, page after page (`GET /api/v1/boards/scans`). */
export function useScans(board: string | null) {
  return useInfiniteQuery({
    queryKey: scansKey(board),
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam, signal }): Promise<ScanPage> =>
      unwrap(
        await api.GET("/api/v1/boards/scans", {
          params: {
            query: {
              ...(board === null ? {} : { board }),
              ...(pageParam === null ? {} : { cursor: pageParam }),
            },
          },
          signal,
        }),
      ),
    getNextPageParam: (last) => last.next_cursor ?? null,
  });
}

/** What the owner can do to a board. */
export type BoardAction =
  | { kind: "approve" }
  | { kind: "pause" }
  | { kind: "check" }
  | { kind: "decline"; reason: string }
  | { kind: "config"; config: Record<string, unknown> };

async function act(id: string, action: BoardAction): Promise<BoardDetail> {
  const path = { params: { path: { id } }, headers: CSRF_HEADERS };
  switch (action.kind) {
    case "approve": {
      return unwrap(await api.POST("/api/v1/boards/{id}/approve", path));
    }
    case "pause": {
      return unwrap(await api.POST("/api/v1/boards/{id}/pause", path));
    }
    case "check": {
      return unwrap(await api.POST("/api/v1/boards/{id}/check", path));
    }
    case "decline": {
      return unwrap(
        await api.POST("/api/v1/boards/{id}/decline", { ...path, body: { reason: action.reason } }),
      );
    }
    case "config": {
      return unwrap(
        await api.PUT("/api/v1/boards/{id}/config", { ...path, body: { config: action.config } }),
      );
    }
  }
}

/** Approve, decline, pause, check or edit a board; the lists follow. */
export function useBoardAction(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (action: BoardAction) => act(id, action),
    onSuccess: (detail) => {
      queryClient.setQueryData(detailKey(id), detail);
      void queryClient.invalidateQueries({ queryKey: BOARDS_QUERY_KEY });
    },
  });
}

/** Suggest a board by its site; the answer is the board (new, or already there). */
export function useSuggestBoard() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (url: string): Promise<BoardDetail> =>
      unwrap(
        await api.POST("/api/v1/boards/suggestions", { body: { url }, headers: CSRF_HEADERS }),
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: BOARDS_QUERY_KEY });
    },
  });
}

/** The problems a board action can answer with, each with its own message. */
export const BOARD_ERRORS = [
  "not_board_owner",
  "not_waiting",
  "not_approved",
  "check_needed",
  "robots_disallowed",
  "config_invalid",
  "invalid_config",
  "invalid_url",
  "suggestion_limit_reached",
] as const;
export type BoardError = (typeof BOARD_ERRORS)[number] | "generic";

/** What to say about an action that failed. */
export function boardErrorOf(error: unknown): BoardError {
  const code = problemCode(error);
  return code !== undefined && (BOARD_ERRORS as readonly string[]).includes(code)
    ? (code as BoardError)
    : "generic";
}

/** A sum of euros a year, rounded to thousands: "€118k". */
export function formatPay(eur: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "EUR",
    notation: "compact",
    maximumFractionDigits: 0,
  }).format(eur);
}

/**
 * A board's salary line: "avg €118k/yr · from 64 of 210 jobs (30% state a salary)", a country
 * benchmark ("≈ €121k/yr (CH benchmark)"), "few salaries", or "no salary data".
 */
export function salaryLine(
  ranking: BoardRanking,
  locale: string,
  t: TFunction<"sources">,
): { text: string; muted: boolean } {
  const pay = (eur: number) => formatPay(eur, locale);
  const share = Math.round(ranking.salary_share * 100);
  const counts = {
    salaried: String(ranking.salaried_jobs),
    relevant: String(ranking.relevant_jobs),
    share: String(share),
  };
  const avg = ranking.avg_salary_eur_year;
  switch (ranking.basis) {
    case "own": {
      return { text: t("boards.salary.own", { avg: pay(avg ?? 0), ...counts }), muted: false };
    }
    case "few": {
      return { text: t("boards.salary.few", { avg: pay(avg ?? 0), ...counts }), muted: true };
    }
    case "benchmark": {
      const benchmark = ranking.benchmark;
      return {
        text: t("boards.salary.benchmark", {
          pay: pay(benchmark?.eur_year ?? 0),
          country: benchmark?.code ?? "",
        }),
        muted: false,
      };
    }
    case "none": {
      return { text: t("boards.salary.none"), muted: true };
    }
  }
}

/** A board's totals: "7 days: 120 read · 4 matched · €0.010". */
export function totalsLine(
  totals: BoardSummary["totals_7d"],
  locale: string,
  label: string,
  t: TFunction<"sources">,
): string {
  return t("boards.totals", {
    label,
    read: String(totals.read),
    matched: String(totals.matched),
    cost: formatEuros(totals.cost_eur, locale),
  });
}
