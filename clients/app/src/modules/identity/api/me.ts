import { queryOptions, useQuery } from "@tanstack/react-query";

import { api, errorStatus, unwrap, type Schema } from "~/lib/api";

/** Who is signed in (`GET /api/v1/me`). Never a token. */
export type Me = Schema<"Me">;

/** A sign-in provider (`campbooks`, `linkedin`). */
export type Provider = Schema<"Provider">;

export const ME_QUERY_KEY = ["identity", "me"] as const;

/**
 * The signed-in person, asked once per page load: it changes only by
 * signing in or out, which reloads it. A 401 is an answer (signed out),
 * never retried.
 */
export const meQuery = queryOptions({
  queryKey: ME_QUERY_KEY,
  queryFn: async ({ signal }): Promise<Me> => unwrap(await api.GET("/api/v1/me", { signal })),
  staleTime: Number.POSITIVE_INFINITY,
  refetchOnWindowFocus: false,
  retry: (failures, error) => errorStatus(error) !== 401 && failures < 1,
});

/** The signed-in person (TanStack Query over `GET /api/v1/me`). */
export function useMe() {
  return useQuery(meQuery);
}
