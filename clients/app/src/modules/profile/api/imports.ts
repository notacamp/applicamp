import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, unwrapEmpty, type Schema } from "~/lib/api";

import { PROFILE_QUERY_KEY, type ImportKind, type ProfileView } from "./profile";

/** A draft waiting for review. */
export type Import = Schema<"Import">;
/** `replace` or `merge`. */
export type ApplyMode = Schema<"ApplyMode">;

/** What one import is made from. */
export type ImportSource =
  | { kind: Extract<ImportKind, "linkedin_zip" | "pdf">; file: File }
  | { kind: Extract<ImportKind, "text">; text: string };

/** The largest file the server takes, per kind. */
export const MAX_FILE_BYTES: Record<"linkedin_zip" | "pdf", number> = {
  linkedin_zip: 20 * 1024 * 1024,
  pdf: 8 * 1024 * 1024,
};
/** The longest text the server takes. */
export const MAX_TEXT_CHARS = 60_000;

/**
 * Uploads `source` (`POST /api/v1/profile/imports`, multipart): the server
 * reads it and answers with a draft. Nothing is saved to the profile.
 */
export async function createImport(source: ImportSource): Promise<Import> {
  return unwrap(
    await api.POST("/api/v1/profile/imports", {
      headers: CSRF_HEADERS,
      body: { kind: source.kind },
      bodySerializer: () => {
        const form = new FormData();
        form.append("kind", source.kind);
        if (source.kind === "text") {
          form.append("text", source.text);
        } else {
          form.append("file", source.file, source.file.name);
        }
        return form;
      },
    }),
  );
}

export function useCreateImport() {
  return useMutation({ mutationFn: createImport });
}

export const importQueryKey = (id: string) => ["profile", "imports", id] as const;

/** One draft (`GET /api/v1/profile/imports/{id}`). */
export function importQuery(id: string) {
  return queryOptions({
    queryKey: importQueryKey(id),
    // A draft never changes on the server: the one the upload answered with stands.
    staleTime: Number.POSITIVE_INFINITY,
    queryFn: async ({ signal }): Promise<Import> =>
      unwrap(await api.GET("/api/v1/profile/imports/{id}", { params: { path: { id } }, signal })),
  });
}

export function useImport(id: string) {
  return useQuery(importQuery(id));
}

/** Applies a draft (Replace or Merge); the page then shows the new profile. */
export function useApplyImport() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, mode }: { id: string; mode: ApplyMode }): Promise<ProfileView> =>
      unwrap(
        await api.POST("/api/v1/profile/imports/{id}/apply", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: { mode },
        }),
      ),
    onSuccess: (view, { id }) => {
      queryClient.setQueryData(PROFILE_QUERY_KEY, view);
      queryClient.removeQueries({ queryKey: importQueryKey(id) });
    },
  });
}

/** Throws a draft away. */
export function useDiscardImport() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      unwrapEmpty(
        await api.POST("/api/v1/profile/imports/{id}/discard", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      );
    },
    onSuccess: (_, id) => {
      queryClient.removeQueries({ queryKey: importQueryKey(id) });
    },
  });
}
