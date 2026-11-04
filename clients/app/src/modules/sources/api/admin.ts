import {
  queryOptions,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import type { TFunction } from "i18next";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

import { toRow, type AdminRow, type ShowFilter } from "./admin-rows";
import { BOARDS_QUERY_KEY, type BoardDetail } from "./boards";

/** The admin page's first answer: totals, this week, and every board not yet read. */
export type AdminOverview = Schema<"AdminOverview">;
/** One line of the table, as the server describes it. */
export type AdminBoard = Schema<"AdminBoard">;
export type AdminList = Schema<"AdminList">;
/** What a board waits on its owner for. */
export type BoardNeed = Schema<"BoardNeed">;
/** The email asking a board for permission, as a draft, and where the request stands. */
export type PermissionEmail = Schema<"PermissionEmail">;
export type RequestKind = Schema<"RequestKind">;

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

/** Take back a decline: the board goes back to where it was. */
export function useReopen(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<BoardDetail> =>
      unwrap(
        await api.POST("/api/v1/boards/{id}/reopen", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: BOARDS_QUERY_KEY });
    },
  });
}

/** The table or the waiting list: its rows so far, and what more there is to ask for. */
export interface AdminListState {
  rows: AdminRow[];
  isPending: boolean;
  isError: boolean;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchNextPage: () => Promise<unknown>;
  refetch: () => Promise<unknown>;
}

/** The boards, 50 a page, searched and filtered by the server (`GET /api/v1/boards/admin/list`). */
export function useAdminList(show: ShowFilter, q: string): AdminListState {
  const query = useInfiniteQuery({
    queryKey: [...BOARDS_QUERY_KEY, "admin-list", show, q] as const,
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam, signal }): Promise<AdminList> =>
      unwrap(
        await api.GET("/api/v1/boards/admin/list", {
          params: {
            query: {
              ...(show === "all" ? {} : { show }),
              ...(q.trim() === "" ? {} : { q: q.trim() }),
              ...(pageParam === null ? {} : { cursor: pageParam }),
            },
          },
          signal,
        }),
      ),
    getNextPageParam: (last) => last.next_cursor ?? null,
  });
  return {
    rows: (query.data?.pages ?? []).flatMap((page) => page.boards.map(toRow)),
    isPending: query.isPending,
    isError: query.isError,
    hasNextPage: query.hasNextPage,
    isFetchingNextPage: query.isFetchingNextPage,
    fetchNextPage: query.fetchNextPage,
    refetch: query.refetch,
  };
}

/** How a board is read, in plain words; a kind this client does not know reads as a list. */
export function howRead(kind: string, t: TFunction<"sources">): string {
  switch (kind) {
    case "rss": {
      return t("admin.how.rss");
    }
    case "jsonld_sitemap": {
      return t("admin.how.jsonld_sitemap");
    }
    case "hn": {
      return t("admin.how.hn");
    }
    case "company_page": {
      return t("admin.how.company_page");
    }
    case "hiring_system": {
      return t("admin.how.hiring_system");
    }
    default: {
      return t("admin.how.json_api");
    }
  }
}
