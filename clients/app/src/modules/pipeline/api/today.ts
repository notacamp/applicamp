import { queryOptions, useQuery } from "@tanstack/react-query";

import { api, unwrap, type Schema } from "~/lib/api";

import { TODAY_QUERY_KEY } from "./opportunities";

/** Where the person is: profile, goals, new matches, the last search and the next step. */
export type Today = Schema<"Today">;
export type NextStep = Schema<"NextStep">;
export type TodaySearch = Schema<"TodaySearch">;

/** `GET /api/v1/today`. */
export const todayQuery = queryOptions({
  queryKey: TODAY_QUERY_KEY,
  queryFn: async ({ signal }): Promise<Today> => unwrap(await api.GET("/api/v1/today", { signal })),
});

export function useToday() {
  return useQuery(todayQuery);
}
