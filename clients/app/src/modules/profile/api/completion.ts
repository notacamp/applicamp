import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, unwrapEmpty, type Schema } from "~/lib/api";

import { PROFILE_QUERY_KEY, type ProfileView } from "./profile";

/** How strong the profile is, and what would raise it (`GET /api/v1/profile/strength`). */
export type StrengthView = Schema<"StrengthView">;
export type StrengthItem = Schema<"StrengthItem">;
export type StrengthKey = Schema<"StrengthKey">;
/** The AI's questions about the profile's gaps. */
export type QuestionsView = Schema<"QuestionsView">;
export type Question = Schema<"Question">;
export type QuestionSection = Schema<"QuestionSection">;
export type ReviewStatus = Schema<"ReviewStatus">;
/** A draft made from an answer, waiting to be kept. */
export type DraftView = Schema<"DraftView">;
export type KeepRequest = Schema<"KeepRequest">;
/** Skills found in the person's own positions. */
export type SuggestedSkills = Schema<"SuggestedSkills">;
export type SuggestedSkill = Schema<"SuggestedSkill">;

/** Today's key (the pipeline module's): Today shows the strength, so it is read again. */
const TODAY_QUERY_KEY = ["today"] as const;
export const STRENGTH_QUERY_KEY = ["profile", "strength"] as const;
export const QUESTIONS_QUERY_KEY = ["profile", "questions"] as const;
export const SKILLS_QUERY_KEY = ["profile", "skills"] as const;

/** How often a running review is asked about again. */
const POLL_MS = 2000;

/** The strength (`GET /api/v1/profile/strength`). */
export const strengthQuery = queryOptions({
  queryKey: STRENGTH_QUERY_KEY,
  queryFn: async ({ signal }): Promise<StrengthView> =>
    unwrap(await api.GET("/api/v1/profile/strength", { signal })),
});

export function useStrength() {
  return useQuery(strengthQuery);
}

/**
 * The questions (`GET /api/v1/profile/questions`). The first ask for a
 * profile version starts the AI review (202 `running`); this asks again
 * every two seconds until it is made.
 */
export function useQuestions() {
  return useQuery({
    queryKey: QUESTIONS_QUERY_KEY,
    queryFn: async ({ signal }): Promise<QuestionsView> =>
      unwrap(await api.GET("/api/v1/profile/questions", { signal })),
    refetchInterval: (query) => (query.state.data?.status === "running" ? POLL_MS : false),
  });
}

/** The skills found in the roles (`GET /api/v1/profile/skills/suggested`), polled while the review runs. */
export function useSuggestedSkills() {
  return useQuery({
    queryKey: SKILLS_QUERY_KEY,
    queryFn: async ({ signal }): Promise<SuggestedSkills> =>
      unwrap(await api.GET("/api/v1/profile/skills/suggested", { signal })),
    refetchInterval: (query) => (query.state.data?.status === "running" ? POLL_MS : false),
  });
}

/** Everything that shows the strength is read again. */
function useRefreshStrength() {
  const queryClient = useQueryClient();
  return () => {
    void queryClient.invalidateQueries({ queryKey: STRENGTH_QUERY_KEY });
    void queryClient.invalidateQueries({ queryKey: QUESTIONS_QUERY_KEY });
    void queryClient.invalidateQueries({ queryKey: SKILLS_QUERY_KEY });
    void queryClient.invalidateQueries({ queryKey: TODAY_QUERY_KEY });
  };
}

/** Tries the review again after it failed (`POST /api/v1/profile/questions/review`). */
export function useReviewAgain() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<QuestionsView> =>
      unwrap(await api.POST("/api/v1/profile/questions/review", { headers: CSRF_HEADERS })),
    onSuccess: (view) => {
      queryClient.setQueryData(QUESTIONS_QUERY_KEY, view);
      void queryClient.invalidateQueries({ queryKey: SKILLS_QUERY_KEY });
    },
  });
}

/** Skips a question (`POST /api/v1/profile/questions/{id}/skip`). */
export function useSkipQuestion() {
  const refresh = useRefreshStrength();
  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      unwrapEmpty(
        await api.POST("/api/v1/profile/questions/{id}/skip", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      );
    },
    onSuccess: refresh,
  });
}

/** Makes a draft from an answer (`POST /api/v1/profile/questions/{id}/draft`). Nothing is saved. */
export function useDraftAnswer() {
  return useMutation({
    mutationFn: async ({ id, answer }: { id: string; answer: string }): Promise<DraftView> =>
      unwrap(
        await api.POST("/api/v1/profile/questions/{id}/draft", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: { answer },
        }),
      ),
  });
}

/** Keeps a draft, as it is or edited (`POST /api/v1/profile/drafts/{id}/keep`). */
export function useKeepDraft() {
  const queryClient = useQueryClient();
  const refresh = useRefreshStrength();
  return useMutation({
    mutationFn: async ({ id, ...keep }: { id: string } & KeepRequest): Promise<ProfileView> =>
      unwrap(
        await api.POST("/api/v1/profile/drafts/{id}/keep", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: keep,
        }),
      ),
    onSuccess: (view) => {
      queryClient.setQueryData(PROFILE_QUERY_KEY, view);
      refresh();
    },
  });
}

/** Throws a draft away (`POST /api/v1/profile/drafts/{id}/discard`). */
export function useDiscardDraft() {
  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      unwrapEmpty(
        await api.POST("/api/v1/profile/drafts/{id}/discard", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
        }),
      );
    },
  });
}

/** Adds a suggested skill to the profile, at the end of its skills. */
export function useAddSkill() {
  const queryClient = useQueryClient();
  const refresh = useRefreshStrength();
  return useMutation({
    mutationFn: async (name: string): Promise<SuggestedSkills> =>
      unwrap(
        await api.POST("/api/v1/profile/skills/suggested/{name}/add", {
          params: { path: { name } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: (skills) => {
      queryClient.setQueryData(SKILLS_QUERY_KEY, skills);
      void queryClient.invalidateQueries({ queryKey: PROFILE_QUERY_KEY });
      refresh();
    },
  });
}

/** Says a suggested skill is not theirs: it is not suggested again. */
export function useDismissSkill() {
  const queryClient = useQueryClient();
  const refresh = useRefreshStrength();
  return useMutation({
    mutationFn: async (name: string): Promise<SuggestedSkills> =>
      unwrap(
        await api.POST("/api/v1/profile/skills/suggested/{name}/dismiss", {
          params: { path: { name } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: (skills) => {
      queryClient.setQueryData(SKILLS_QUERY_KEY, skills);
      refresh();
    },
  });
}

/** Puts the skills in a new order (`PUT /api/v1/profile/skills/order`). */
export function useOrderSkills() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (names: string[]): Promise<ProfileView> =>
      unwrap(
        await api.PUT("/api/v1/profile/skills/order", {
          headers: CSRF_HEADERS,
          body: { names },
        }),
      ),
    onSuccess: (view) => {
      queryClient.setQueryData(PROFILE_QUERY_KEY, view);
    },
  });
}

/** What a part's action asks for. */
export type Action =
  | { kind: "improve" }
  | { kind: "question"; id: string }
  | { kind: "skills" }
  | { kind: "import" }
  | { kind: "goals" }
  | { kind: "edit"; section: "basics" | "work" | "education" | "languages" };

/** The strength item's `action`, as the page acts on it. */
export function parseAction(action: string | null | undefined): Action | null {
  if (action === undefined || action === null) {
    return null;
  }
  if (action === "skills" || action === "import" || action === "goals") {
    return { kind: action };
  }
  if (action.startsWith("question:")) {
    return { kind: "question", id: action.slice("question:".length) };
  }
  if (action.startsWith("edit:")) {
    const section = action.slice("edit:".length);
    if (
      section === "basics" ||
      section === "work" ||
      section === "education" ||
      section === "languages"
    ) {
      return { kind: "edit", section };
    }
  }
  return null;
}

/** `"2021-03"` or `"2021"` as the year. */
export function yearOf(date: string | null | undefined): string | undefined {
  return date === undefined || date === null || date === "" ? undefined : date.slice(0, 4);
}
