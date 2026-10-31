import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

import { QUESTIONS_QUERY_KEY, STRENGTH_QUERY_KEY } from "./completion";
import { PROFILE_QUERY_KEY } from "./profile";

/** What Applicamp knows and assumes about you (`GET /api/v1/profile/about`). */
export type AboutView = Schema<"AboutView">;
export type AboutSentence = Schema<"SentenceView">;
export type AboutEmployer = Schema<"EmployerView">;
export type AboutChange = Schema<"ChangeView">;
export type AboutChoice = Schema<"Choice">;
export type AboutAccuracyValue = Schema<"Accuracy">;
export type AboutFlag = Schema<"Flag">;
/** What Save would do (`GET /api/v1/profile/about/review`). */
export type AboutReview = Schema<"AboutReview">;
export type ReviewGroup = Schema<"ReviewGroup">;
export type ReviewRow = Schema<"ReviewRow">;
export type AboutSaved = Schema<"Saved">;

/** Longest "Anything else", in characters (the server's limit). */
export const MAX_EXTRA = 4000;

export const ABOUT_QUERY_KEY = ["profile", "about"] as const;
const REVIEW_QUERY_KEY = ["profile", "about", "review"] as const;

/** The wizard's state. Reading it never calls the model. */
export const aboutQuery = queryOptions({
  queryKey: ABOUT_QUERY_KEY,
  queryFn: async ({ signal }): Promise<AboutView> =>
    unwrap(await api.GET("/api/v1/profile/about", { signal })),
});

export function useAbout() {
  return useQuery(aboutQuery);
}

/** What Save would do, read again whenever the wizard's state changes. */
export function useAboutReview(enabled: boolean) {
  return useQuery({
    queryKey: REVIEW_QUERY_KEY,
    enabled,
    queryFn: async ({ signal }): Promise<AboutReview> =>
      unwrap(await api.GET("/api/v1/profile/about/review", { signal })),
  });
}

/** Every change to the wizard's state is stored, and what Save would do is read again. */
function useStore() {
  const queryClient = useQueryClient();
  return (view: AboutView) => {
    queryClient.setQueryData(ABOUT_QUERY_KEY, view);
    void queryClient.invalidateQueries({ queryKey: REVIEW_QUERY_KEY });
  };
}

/** Makes the portrait (one model call), or takes the one made for this profile version. */
export function useMakePortrait(language: string) {
  const store = useStore();
  return useMutation({
    mutationFn: async (again: boolean): Promise<AboutView> =>
      unwrap(
        await api.POST("/api/v1/profile/about/portrait", {
          params: { query: { locale: language, again } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** "That's right", "That's wrong", "Ask me later" (none takes it back). */
export function useChoose() {
  const store = useStore();
  return useMutation({
    mutationFn: async ({
      id,
      choice,
    }: {
      id: string;
      choice: AboutChoice | null;
    }): Promise<AboutView> =>
      unwrap(
        await api.PUT("/api/v1/profile/about/sentences/{id}", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: { choice },
        }),
      ),
    onSuccess: store,
  });
}

/** "Is this accurate?". */
export function useAccuracy() {
  const store = useStore();
  return useMutation({
    mutationFn: async (accuracy: AboutAccuracyValue): Promise<AboutView> =>
      unwrap(
        await api.PUT("/api/v1/profile/about/accuracy", {
          headers: CSRF_HEADERS,
          body: { accuracy },
        }),
      ),
    onSuccess: store,
  });
}

/** Types a past employer's own website (nothing is read until the portrait is made again). */
export function useEmployerSite() {
  const store = useStore();
  return useMutation({
    mutationFn: async ({ position, url }: { position: number; url: string }): Promise<AboutView> =>
      unwrap(
        await api.PUT("/api/v1/profile/about/employers/{position}/site", {
          params: { path: { position } },
          headers: CSRF_HEADERS,
          body: { url },
        }),
      ),
    onSuccess: store,
  });
}

/** "Anything else": one model call turns the text into proposed changes. */
export function useSubmitExtra(language: string) {
  const store = useStore();
  return useMutation({
    mutationFn: async (text: string): Promise<AboutView> =>
      unwrap(
        await api.POST("/api/v1/profile/about/extra", {
          params: { query: { locale: language } },
          headers: CSRF_HEADERS,
          body: { text },
        }),
      ),
    onSuccess: store,
  });
}

/** What the person does with a change: it's true, edit it, or use their own words. */
export type ChangeAction =
  { action: "true" } | { action: "edit"; value: string } | { action: "own_words" };

export function useSettleChange() {
  const store = useStore();
  return useMutation({
    mutationFn: async ({ id, ...body }: { id: string } & ChangeAction): Promise<AboutView> =>
      unwrap(
        await api.PUT("/api/v1/profile/about/changes/{id}", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body,
        }),
      ),
    onSuccess: store,
  });
}

/** Takes a change out of what will be saved. */
export function useRemoveChange() {
  const store = useStore();
  return useMutation({
    mutationFn: async (id: string): Promise<AboutView> =>
      unwrap(
        await api.DELETE("/api/v1/profile/about/changes/{id}", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** Saves everything in one write; the profile and everything that reads it is read again. */
export function useSaveAbout() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<AboutSaved> =>
      unwrap(await api.POST("/api/v1/profile/about/save", { headers: CSRF_HEADERS })),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PROFILE_QUERY_KEY, exact: true });
      void queryClient.invalidateQueries({ queryKey: STRENGTH_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: QUESTIONS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: ["today"] });
      void queryClient.invalidateQueries({ queryKey: ABOUT_QUERY_KEY });
    },
  });
}
