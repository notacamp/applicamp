import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

import { STRENGTH_QUERY_KEY } from "./completion";
import { PROFILE_QUERY_KEY } from "./profile";
import type { FoundKey } from "./sections";

/** The facts proposed for the profile (`GET /api/v1/profile/facts/proposals`). */
export type FactProposalsView = Schema<"FactProposalsView">;
export type FactGroup = Schema<"FactGroup">;
export type FactProposal = Schema<"FactProposalView">;
/** `you_said`, `you_confirmed`, `your_website` or `your_cv`. */
export type FactSource = Schema<"FactSource">;
/** `open`, `kept` or `dropped`. */
export type FactProposalState = Schema<"FactProposalState">;
/** What an import applied in the last day found. */
export type CvRead = Schema<"CvRead">;

export const FACT_PROPOSALS_QUERY_KEY = ["profile", "facts", "proposals"] as const;

/** The proposals: open ones, and those decided in the last day. Never calls a model. */
export const factProposalsQuery = queryOptions({
  queryKey: FACT_PROPOSALS_QUERY_KEY,
  queryFn: async ({ signal }): Promise<FactProposalsView> =>
    unwrap(await api.GET("/api/v1/profile/facts/proposals", { signal })),
});

export function useFactProposals() {
  return useQuery(factProposalsQuery);
}

/** What a decision answers with: the proposals now; the profile changed too. */
function useStore() {
  const queryClient = useQueryClient();
  return (view: FactProposalsView) => {
    queryClient.setQueryData(FACT_PROPOSALS_QUERY_KEY, view);
    // The profile itself (not every query under it, these proposals included).
    void queryClient.invalidateQueries({ queryKey: PROFILE_QUERY_KEY, exact: true });
    void queryClient.invalidateQueries({ queryKey: STRENGTH_QUERY_KEY });
  };
}

/** One decision on one proposal. */
export type FactDecision = "keep" | "drop" | "undo";

/** Keep, drop, or undo the last of these, on one proposal. */
export function useDecideFact() {
  const store = useStore();
  return useMutation({
    mutationFn: async ({
      id,
      decision,
    }: {
      id: string;
      decision: FactDecision;
    }): Promise<FactProposalsView> => {
      const options = { params: { path: { id } }, headers: CSRF_HEADERS };
      switch (decision) {
        case "keep": {
          return unwrap(await api.POST("/api/v1/profile/facts/proposals/{id}/keep", options));
        }
        case "drop": {
          return unwrap(await api.POST("/api/v1/profile/facts/proposals/{id}/drop", options));
        }
        case "undo": {
          return unwrap(await api.POST("/api/v1/profile/facts/proposals/{id}/undo", options));
        }
      }
    },
    onSuccess: store,
  });
}

/** Keep every open proposal: one new profile version. */
export function useKeepAllFacts() {
  const store = useStore();
  return useMutation({
    mutationFn: async (): Promise<FactProposalsView> =>
      unwrap(await api.POST("/api/v1/profile/facts/proposals/keep-all", { headers: CSRF_HEADERS })),
    onSuccess: store,
  });
}

/** Undo the last Keep all: every fact it kept waits again. One profile version. */
export function useUndoKeepAll() {
  const store = useStore();
  return useMutation({
    mutationFn: async (id: string): Promise<FactProposalsView> =>
      unwrap(
        await api.POST("/api/v1/profile/facts/proposals/batches/{id}/undo", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** How many proposals there are, and how many are decided. */
export function progressOf(view: FactProposalsView): { decided: number; total: number } {
  const decided = view.kept + view.dropped;
  return { decided, total: decided + view.open };
}

/** What a CV read again found, part by part, the empty ones left out. */
export function foundInRead(read: CvRead): { key: FoundKey; count: number }[] {
  const counts: { key: FoundKey; count: number }[] = [
    { key: "jobs", count: read.jobs },
    { key: "achievements", count: read.achievements },
    { key: "skills", count: read.skills },
    { key: "degrees", count: read.degrees },
    { key: "languages", count: read.languages },
    { key: "certificates", count: read.certificates },
    { key: "projects", count: read.projects },
  ];
  return counts.filter(({ count }) => count > 0);
}
