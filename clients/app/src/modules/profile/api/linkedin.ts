import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, unwrapEmpty, type Schema } from "~/lib/api";

/** Where the check-up is (`GET /api/v1/profile/linkedin`). */
export type LinkedInView = Schema<"LinkedInView">;
export type LinkedInReview = Schema<"LinkedInReview">;
export type LinkedInFinding = Schema<"LinkedInFindingView">;
export type LinkedInProposal = Schema<"LinkedInProposal">;
export type LinkedInFlag = Schema<"LinkedInFlag">;
/** `linkedin_zip` or `pdf`. */
export type LinkedInKind = Schema<"LinkedInKind">;
export type ProposalStatus = Schema<"ProposalStatus">;
export type ProposalDecision = Schema<"ProposalDecision">;

/** The largest file the server takes, per kind. */
export const MAX_LINKEDIN_BYTES: Record<LinkedInKind, number> = {
  linkedin_zip: 20 * 1024 * 1024,
  pdf: 8 * 1024 * 1024,
};

export const LINKEDIN_QUERY_KEY = ["profile", "linkedin"] as const;

/** The check-up's state. Reading it never calls the model. */
export const linkedInQuery = queryOptions({
  queryKey: LINKEDIN_QUERY_KEY,
  queryFn: async ({ signal }): Promise<LinkedInView> =>
    unwrap(await api.GET("/api/v1/profile/linkedin", { signal })),
});

export function useLinkedIn() {
  return useQuery(linkedInQuery);
}

function useStore() {
  const queryClient = useQueryClient();
  return (view: LinkedInView) => {
    queryClient.setQueryData(LINKEDIN_QUERY_KEY, view);
  };
}

/**
 * Uploads the LinkedIn profile (`POST /api/v1/profile/linkedin`, multipart):
 * the server reads it (never keeps the file) and makes the review with one
 * model call. Nothing is written to the profile.
 */
export function useCheckLinkedIn(language: string) {
  const store = useStore();
  return useMutation({
    mutationFn: async ({ kind, file }: { kind: LinkedInKind; file: File }): Promise<LinkedInView> =>
      unwrap(
        await api.POST("/api/v1/profile/linkedin", {
          params: { query: { locale: language } },
          headers: CSRF_HEADERS,
          body: { kind, file: file.name },
          bodySerializer: () => {
            const form = new FormData();
            form.append("kind", kind);
            form.append("file", file, file.name);
            return form;
          },
        }),
      ),
    onSuccess: store,
  });
}

/** Keep, drop, mark done, take back, or rewrite one proposal. */
export function useDecideProposal() {
  const store = useStore();
  return useMutation({
    mutationFn: async ({
      id,
      decision,
    }: {
      id: string;
      decision: ProposalDecision;
    }): Promise<LinkedInView> =>
      unwrap(
        await api.PATCH("/api/v1/profile/linkedin/proposals/{id}", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: decision,
        }),
      ),
    onSuccess: store,
  });
}

/** Start over: the review is thrown away. */
export function useStartOver() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<void> => {
      unwrapEmpty(await api.DELETE("/api/v1/profile/linkedin", { headers: CSRF_HEADERS }));
    },
    onSuccess: () => {
      queryClient.setQueryData(LINKEDIN_QUERY_KEY, (current: LinkedInView | undefined) =>
        current === undefined ? current : { ...current, review: null },
      );
      void queryClient.invalidateQueries({ queryKey: LINKEDIN_QUERY_KEY });
    },
  });
}
