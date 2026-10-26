import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, problemCode, unwrap, type Schema } from "~/lib/api";

/** A search: where it is, what it read, scored and added, what it cost. */
export type Search = Schema<"Search">;
export type SearchStatus = Schema<"SearchStatus">;
export type SearchStop = Schema<"SearchStop">;
export type SearchEstimate = Schema<"SearchEstimate">;
/** What the account spent on AI, and its limits. */
export type Spend = Schema<"Spend">;

export const SEARCHES_QUERY_KEY = ["searches"] as const;
const LATEST_KEY = [...SEARCHES_QUERY_KEY, "latest"] as const;
const ESTIMATE_KEY = [...SEARCHES_QUERY_KEY, "estimate"] as const;

/** How often a running search is asked how far it got. */
export const PROGRESS_POLL_MS = 1500;

/** Whether `search` is still going. */
export function isRunning(search: Pick<Search, "status"> | null | undefined): boolean {
  return search?.status === "queued" || search?.status === "running";
}

/** The latest search (`GET /api/v1/searches/latest`), polled while it runs. */
export const latestSearchQuery = queryOptions({
  queryKey: LATEST_KEY,
  queryFn: async ({ signal }): Promise<Search | null> =>
    unwrap(await api.GET("/api/v1/searches/latest", { signal })).search ?? null,
  refetchInterval: (query) => (isRunning(query.state.data) ? PROGRESS_POLL_MS : false),
});

export function useLatestSearch() {
  return useQuery(latestSearchQuery);
}

/** What the next search would cost, and what was spent (`GET /api/v1/searches/estimate`). */
export const estimateQuery = queryOptions({
  queryKey: ESTIMATE_KEY,
  queryFn: async ({ signal }): Promise<SearchEstimate> =>
    unwrap(await api.GET("/api/v1/searches/estimate", { signal })),
});

export function useEstimate() {
  return useQuery(estimateQuery);
}

/** Starts a search (`POST /api/v1/searches`); the latest search is then the new one. */
export function useStartSearch() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<Search> =>
      unwrap(await api.POST("/api/v1/searches", { headers: CSRF_HEADERS })),
    onSuccess: (search) => {
      queryClient.setQueryData(LATEST_KEY, search);
      void queryClient.invalidateQueries({ queryKey: ESTIMATE_KEY });
    },
  });
}

/** The problems starting a search can answer with, each with its own message. */
export const SEARCH_ERRORS = [
  "profile_needed",
  "search_running",
  "search_limit_reached",
  "ai_unavailable",
  "ai_budget_reached",
  "ai_daily_limit_reached",
  "ai_failed",
] as const;
export type SearchError = (typeof SEARCH_ERRORS)[number] | "generic";

function isSearchError(code: string | undefined): code is (typeof SEARCH_ERRORS)[number] {
  return code !== undefined && (SEARCH_ERRORS as readonly string[]).includes(code);
}

/** What to say about a search that could not start. */
export function searchErrorOf(error: unknown): SearchError {
  const code = problemCode(error);
  return isSearchError(code) ? code : "generic";
}

/** A sum of euros in `locale`: "€0.42", "0,42 €". Small sums keep their cents. */
export function formatEuros(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "EUR",
    minimumFractionDigits: value > 0 && value < 0.01 ? 3 : 2,
    maximumFractionDigits: value > 0 && value < 0.01 ? 3 : 2,
  }).format(value);
}

/** A limit in euros: whole euros without cents ("€3"), else with them ("€0.30"). */
export function formatLimit(value: number, locale: string): string {
  const whole = Number.isInteger(value);
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "EUR",
    minimumFractionDigits: whole ? 0 : 2,
    maximumFractionDigits: 2,
  }).format(value);
}
