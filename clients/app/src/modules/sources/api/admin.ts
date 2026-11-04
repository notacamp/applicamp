import { useMutation, useQuery, useQueryClient, queryOptions } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

import { BOARDS_QUERY_KEY, type BoardDetail, type BoardSummary } from "./boards";

/** The admin page's first answer: totals, this week, and every board not yet read. */
export type AdminOverview = Schema<"AdminOverview">;
export type AdminCandidate = Schema<"AdminCandidate">;
/** What a board waits on its owner for. */
export type BoardNeed = Schema<"BoardNeed">;
/** The email asking a board for permission, as a draft, and where the request stands. */
export type PermissionEmail = Schema<"PermissionEmail">;
export type RequestKind = Schema<"RequestKind">;

/**
 * What the board finder (#75) adds to a board. Absent until its change is merged and the API
 * types are generated again: every one is read as optional and never required.
 */
export interface FinderFields {
  tags?: readonly string[] | null | undefined;
  found_through?: string | null | undefined;
  terms_verdict?: string | null | undefined;
  terms_quote?: string | null | undefined;
  terms_url?: string | null | undefined;
  contact_email?: string | null | undefined;
}

/** A board with what the finder may have added. */
export function withFinder(board: BoardSummary): BoardSummary & FinderFields {
  return board;
}

const ADMIN_KEY = [...BOARDS_QUERY_KEY, "admin"] as const;
const emailKey = (id: string) => [...BOARDS_QUERY_KEY, "permission-email", id] as const;

/** The overview (`GET /api/v1/boards/admin`); a person who is not an owner gets a 404. */
export function useAdminOverview(enabled: boolean) {
  return useQuery(
    queryOptions({
      queryKey: ADMIN_KEY,
      enabled,
      retry: false,
      queryFn: async ({ signal }): Promise<AdminOverview> =>
        unwrap(await api.GET("/api/v1/boards/admin", { signal })),
    }),
  );
}

/** The permission email for a board; none while `id` is null. */
export function usePermissionEmail(id: string | null) {
  return useQuery({
    queryKey: emailKey(id ?? ""),
    enabled: id !== null,
    queryFn: async ({ signal }): Promise<PermissionEmail> =>
      unwrap(
        await api.GET("/api/v1/boards/{id}/permission-email", {
          params: { path: { id: id ?? "" } },
          signal,
        }),
      ),
  });
}

/** "I sent it": the date is kept. */
export function useMarkAsked(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<PermissionEmail> =>
      unwrap(
        await api.POST("/api/v1/boards/{id}/permission/asked", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: (email) => {
      queryClient.setQueryData(emailKey(id), email);
      void queryClient.invalidateQueries({ queryKey: BOARDS_QUERY_KEY });
    },
  });
}

/** Their answer: yes approves the board, no declines it. */
export function useAnswerRequest(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: { answer: "yes" | "no"; kind?: RequestKind }): Promise<BoardDetail> =>
      unwrap(
        await api.POST("/api/v1/boards/{id}/permission/answer", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: { answer: input.answer, kind: input.kind ?? "permission" },
        }),
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: BOARDS_QUERY_KEY });
    },
  });
}
