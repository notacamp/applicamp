import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

/** The goals with the version they are at (`GET /api/v1/goals`). */
export type GoalsView = Schema<"GoalsView">;
/** The answers to the interview; every one optional. */
export type Goals = Schema<"Goals">;
export type Seniority = Schema<"Seniority">;
export type WorkMode = Schema<"WorkMode">;
export type Relocate = Schema<"Relocate">;
export type CompanyStage = Schema<"CompanyStage">;
export type Autonomy = Schema<"Autonomy">;
export type Period = Schema<"Period">;
export type CashFloor = Schema<"CashFloor">;

export const GOALS_QUERY_KEY = ["goals"] as const;

/** The signed-in person's goals. */
export const goalsQuery = queryOptions({
  queryKey: GOALS_QUERY_KEY,
  queryFn: async ({ signal }): Promise<GoalsView> =>
    unwrap(await api.GET("/api/v1/goals", { signal })),
});

export function useGoals() {
  return useQuery(goalsQuery);
}

/** Nothing answered, as the server sends it. */
export const EMPTY_GOALS: Goals = {
  roles: [],
  seniority: [],
  work_mode: [],
  locations: [],
  company_stage: [],
  avoid: [],
  must_have: [],
  languages_for_work: [],
};

/** Saves all the answers (`PUT /api/v1/goals`); every save is a new version. */
export function useSaveGoals() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (answers: Goals): Promise<GoalsView> =>
      unwrap(await api.PUT("/api/v1/goals", { headers: CSRF_HEADERS, body: answers })),
    onSuccess: (view) => {
      queryClient.setQueryData(GOALS_QUERY_KEY, view);
    },
  });
}
