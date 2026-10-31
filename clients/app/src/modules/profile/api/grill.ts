import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

import { PROFILE_QUERY_KEY } from "./profile";

/** Every role with how far its interview is (`GET /api/v1/profile/grill`). */
export type GrillList = Schema<"GrillList">;
export type GrillRoleItem = Schema<"GrillRoleItem">;
/** One role's interview (`GET /api/v1/profile/grill/{position}`). */
export type GrillView = Schema<"GrillView">;
export type GrillTurn = Schema<"GrillTurn">;
export type GrillRole = Schema<"GrillRole">;
export type GrillStatus = Schema<"GrillStatus">;
/** A fact taken from an answer: waiting, kept or dropped. */
export type FactProposal = Schema<"FactProposal">;
/** The seven things the interview covers, in order. */
export type Area = Schema<"Area">;
export type CoverageLevel = Schema<"CoverageLevel">;
/** A fact the person kept, on a position of the profile. */
export type ProfileFact = Schema<"ProfileFact">;

/** The areas, in the order they are covered. */
export const AREAS = [
  "context",
  "your_part",
  "stack_scale",
  "results",
  "hard_problems",
  "people",
  "stories",
] as const satisfies readonly Area[];

/** Longest answer the server takes. */
export const MAX_ANSWER = 2000;

export const GRILL_QUERY_KEY = ["profile", "grill"] as const;
const roleKey = (position: number) => [...GRILL_QUERY_KEY, position] as const;
const STRENGTH_QUERY_KEY = ["profile", "strength"] as const;
const QUESTIONS_QUERY_KEY = ["profile", "questions"] as const;

/** Every role with its coverage (`GET /api/v1/profile/grill`). Never calls the model. */
export const grillRolesQuery = queryOptions({
  queryKey: GRILL_QUERY_KEY,
  queryFn: async ({ signal }): Promise<GrillList> =>
    unwrap(await api.GET("/api/v1/profile/grill", { signal })),
});

export function useGrillRoles() {
  return useQuery(grillRolesQuery);
}

/** One role's interview. Nothing is asked of the model by reading it. */
export function useGrillRole(position: number | undefined) {
  return useQuery({
    queryKey: roleKey(position ?? -1),
    enabled: position !== undefined,
    queryFn: async ({ signal }): Promise<GrillView> =>
      unwrap(
        await api.GET("/api/v1/profile/grill/{position}", {
          params: { path: { position: position ?? 0 } },
          signal,
        }),
      ),
  });
}

/** What every change to an interview refreshes: the role itself, and the list's counts. */
function useStore() {
  const queryClient = useQueryClient();
  return (view: GrillView) => {
    queryClient.setQueryData(roleKey(view.position), view);
    void queryClient.invalidateQueries({ queryKey: GRILL_QUERY_KEY, exact: true });
  };
}

/** Kept facts change the profile, so everything that reads it is read again. */
function useStoreKept() {
  const queryClient = useQueryClient();
  const store = useStore();
  return (view: GrillView) => {
    store(view);
    void queryClient.invalidateQueries({ queryKey: PROFILE_QUERY_KEY, exact: true });
    void queryClient.invalidateQueries({ queryKey: STRENGTH_QUERY_KEY });
    void queryClient.invalidateQueries({ queryKey: QUESTIONS_QUERY_KEY });
  };
}

/** Starts (or takes up) a role: one model call, none when a question waits. */
export function useStartGrill(language: string) {
  const store = useStore();
  return useMutation({
    mutationFn: async (position: number): Promise<GrillView> =>
      unwrap(
        await api.POST("/api/v1/profile/grill/{position}/start", {
          params: { path: { position }, query: { locale: language } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** Answers the question waiting: one model call. */
export function useAnswerGrill(language: string) {
  const store = useStore();
  return useMutation({
    mutationFn: async ({ position, text }: { position: number; text: string }) =>
      unwrap(
        await api.POST("/api/v1/profile/grill/{position}/answer", {
          params: { path: { position }, query: { locale: language } },
          headers: CSRF_HEADERS,
          body: { text },
        }),
      ),
    onSuccess: store,
  });
}

/** Skips the question waiting: one model call, or none when nothing is left to ask. */
export function useSkipGrill(language: string) {
  const store = useStore();
  return useMutation({
    mutationFn: async (position: number): Promise<GrillView> =>
      unwrap(
        await api.POST("/api/v1/profile/grill/{position}/skip", {
          params: { path: { position }, query: { locale: language } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** Leaves the role for now. No model. */
export function useDoneGrill() {
  const store = useStore();
  return useMutation({
    mutationFn: async (position: number): Promise<GrillView> =>
      unwrap(
        await api.POST("/api/v1/profile/grill/{position}/done", {
          params: { path: { position } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** Keeps a fact as it is, or fixed (`text`). The quote never changes. */
export function useKeepFact() {
  const store = useStoreKept();
  return useMutation({
    mutationFn: async ({ id, text }: { id: string; text?: string | undefined }) =>
      unwrap(
        await api.POST("/api/v1/profile/grill/proposals/{id}/keep", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: text === undefined ? {} : { text },
        }),
      ),
    onSuccess: store,
  });
}

/** Keeps every fact waiting on a role. */
export function useKeepAllFacts() {
  const store = useStoreKept();
  return useMutation({
    mutationFn: async (position: number): Promise<GrillView> =>
      unwrap(
        await api.POST("/api/v1/profile/grill/{position}/keep-all", {
          params: { path: { position } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** Drops a fact: it is not kept and not shown again. */
export function useDropFact() {
  const store = useStore();
  return useMutation({
    mutationFn: async (id: string): Promise<GrillView> =>
      unwrap(
        await api.POST("/api/v1/profile/grill/proposals/{id}/drop", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** `"2021-03"` or `"2021"` as the year. */
function yearOf(date: string | null | undefined): string | undefined {
  return date === undefined || date === null || date === "" ? undefined : date.slice(0, 4);
}

/** A role as its list entry reads: the title, the company and the years. */
export function describeRole(
  role: GrillRole,
  years: (from: string, to: string | undefined) => string,
): { title: string; meta: string } {
  const title = role.position ?? role.company ?? "";
  const from = yearOf(role.start_date);
  const parts = [
    role.position === null || role.position === undefined ? undefined : role.company,
    from === undefined ? undefined : years(from, yearOf(role.end_date)),
  ].filter((part): part is string => part !== undefined && part !== null && part !== "");
  return { title, meta: parts.join(" · ") };
}
