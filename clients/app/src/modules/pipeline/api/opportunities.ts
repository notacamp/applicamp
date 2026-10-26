import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

/** An opportunity as the list shows it. */
export type OpportunitySummary = Schema<"OpportunitySummary">;
/** One opportunity, whole: why it matches, its gaps, notes, the posting's text. */
export type Opportunity = Schema<"Opportunity">;
export type OpportunityStage = Schema<"OpportunityStage">;
export type OpportunitySource = Schema<"OpportunitySource">;
export type JobWorkMode = Schema<"JobWorkMode">;
/** What can change on an opportunity (`PATCH`). */
export type OpportunityChanges = Schema<"OpportunityChanges">;
/** A pasted link, and what the person typed with it. */
export type NewOpportunity = Schema<"NewOpportunity">;

/** The stages, in the order a job moves through them. */
export const STAGES = [
  "found",
  "shortlisted",
  "applying",
  "applied",
  "talking",
  "interview",
  "offer",
  "closed",
] as const satisfies readonly OpportunityStage[];

/** What the list is filtered by. */
export interface OpportunityFilter {
  stage?: OpportunityStage | undefined;
  /** Words in the role, the company or the place. */
  q?: string | undefined;
  /** Only Found ones never opened. */
  new?: boolean | undefined;
}

/** Every pipeline query starts with this key. */
export const PIPELINE_QUERY_KEY = ["pipeline"] as const;
export const TODAY_QUERY_KEY = ["today"] as const;

const listKey = (filter: OpportunityFilter) =>
  [
    ...PIPELINE_QUERY_KEY,
    "list",
    filter.stage ?? null,
    filter.q ?? "",
    filter.new ?? false,
  ] as const;
export const opportunityKey = (id: string) => [...PIPELINE_QUERY_KEY, "one", id] as const;

/** The opportunities, newest first (`GET /api/v1/opportunities`); "Not for me" ones are left out. */
export function opportunitiesQuery(filter: OpportunityFilter) {
  const q = filter.q?.trim();
  return queryOptions({
    queryKey: listKey(filter),
    queryFn: async ({ signal }): Promise<OpportunitySummary[]> =>
      unwrap(
        await api.GET("/api/v1/opportunities", {
          params: {
            query: {
              ...(filter.stage === undefined ? {} : { stage: filter.stage }),
              ...(q === undefined || q === "" ? {} : { q }),
              ...(filter.new === true ? { new: true } : {}),
            },
          },
          signal,
        }),
      ).opportunities,
  });
}

export function useOpportunities(filter: OpportunityFilter) {
  return useQuery(opportunitiesQuery(filter));
}

/** One opportunity (`GET /api/v1/opportunities/{id}`). */
export function opportunityQuery(id: string) {
  return queryOptions({
    queryKey: opportunityKey(id),
    queryFn: async ({ signal }): Promise<Opportunity> =>
      unwrap(await api.GET("/api/v1/opportunities/{id}", { params: { path: { id } }, signal })),
  });
}

export function useOpportunity(id: string | null) {
  return useQuery({ ...opportunityQuery(id ?? ""), enabled: id !== null });
}

/** Refetch the pipeline and Today (after a search added jobs). */
export async function invalidatePipeline(queryClient: QueryClient): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: PIPELINE_QUERY_KEY }),
    queryClient.invalidateQueries({ queryKey: TODAY_QUERY_KEY }),
  ]);
}

/** Adds a job by its link (`POST /api/v1/opportunities`). */
export function useAddOpportunity() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: NewOpportunity): Promise<Opportunity> =>
      unwrap(await api.POST("/api/v1/opportunities", { headers: CSRF_HEADERS, body })),
    onSuccess: (opportunity) => {
      queryClient.setQueryData(opportunityKey(opportunity.id), opportunity);
      void invalidatePipeline(queryClient);
    },
  });
}

/** Changes an opportunity (`PATCH /api/v1/opportunities/{id}`): its stage, notes, "Not for me". */
export function useUpdateOpportunity() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      id,
      changes,
    }: {
      id: string;
      changes: OpportunityChanges;
    }): Promise<Opportunity> =>
      unwrap(
        await api.PATCH("/api/v1/opportunities/{id}", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: changes,
        }),
      ),
    onSuccess: (opportunity) => {
      queryClient.setQueryData(opportunityKey(opportunity.id), opportunity);
      void invalidatePipeline(queryClient);
    },
  });
}
