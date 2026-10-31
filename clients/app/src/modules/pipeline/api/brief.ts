import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, problemCode, unwrap, type Schema } from "~/lib/api";

import { opportunityKey, PIPELINE_QUERY_KEY } from "./opportunities";

/** What a tailored CV for an opportunity is based on (`GET /api/v1/opportunities/{id}/brief`). */
export type Brief = Schema<"Brief">;
/** A fact read on the company's own site, with the words of the page that say it. */
export type BriefFact = Schema<"BriefFact">;
export type BriefWant = Schema<"BriefWant">;
export type BriefQuestion = Schema<"BriefQuestion">;
export type QuestionState = Schema<"QuestionState">;
/** What an answer came to: facts waiting on a position of the profile. */
export type AnswerResult = Schema<"AnswerResult">;
export type AnswerProposal = Schema<"AnswerProposal">;
export type RoleRef = Schema<"RoleRef">;

const briefKey = (id: string) => [...PIPELINE_QUERY_KEY, "brief", id] as const;

/** Reading the brief never calls the model and never fetches a page. */
export function useBrief(id: string) {
  return useQuery({
    queryKey: briefKey(id),
    queryFn: async ({ signal }): Promise<Brief> =>
      unwrap(
        await api.GET("/api/v1/opportunities/{id}/brief", { params: { path: { id } }, signal }),
      ),
  });
}

/** Every change answers with the brief as it is now. */
function useStore(id: string) {
  const queryClient = useQueryClient();
  return (brief: Brief) => {
    queryClient.setQueryData(briefKey(id), brief);
    // A pasted link gets its posting text on the first read.
    void queryClient.invalidateQueries({ queryKey: opportunityKey(id) });
  };
}

/** Types the company's website: only its host is kept, once, for every job there. */
export function useSetSite(id: string) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (url: string): Promise<Brief> =>
      unwrap(
        await api.PUT("/api/v1/opportunities/{id}/brief/site", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: { url },
        }),
      ),
    onSuccess: store,
  });
}

/**
 * Reads the company's pages (one model call, kept 30 days) and compares the
 * job (one more, kept until the posting changes). `again` reads the company
 * again now.
 */
export function useReadCompany(id: string, language: string) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (again: boolean): Promise<Brief> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/brief/read", {
          params: { path: { id }, query: { locale: language, ...(again ? { again: true } : {}) } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** "Wrong, remove": the fact stays removed on later reads. */
export function useRemoveFact(id: string) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (fact: string): Promise<Brief> =>
      unwrap(
        await api.DELETE("/api/v1/opportunities/{id}/brief/facts/{fact_id}", {
          params: { path: { id, fact_id: fact } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** Undo a removal. */
export function useRestoreFact(id: string) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (fact: string): Promise<Brief> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/brief/facts/{fact_id}/restore", {
          params: { path: { id, fact_id: fact } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** Answers a question: one small model call; the facts wait for Keep, Fix or Drop. */
export function useAnswerQuestion(id: string, language: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ question, text }: { question: string; text: string }) =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/brief/questions/{question_id}/answer", {
          params: { path: { id, question_id: question }, query: { locale: language } },
          headers: CSRF_HEADERS,
          body: { text },
        }),
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: briefKey(id) });
    },
  });
}

/** "I don't have it": the skill is recorded as not theirs. No model call. */
export function useDontHave(id: string) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (question: string): Promise<Brief> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/brief/questions/{question_id}/dont-have", {
          params: { path: { id, question_id: question } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** Skip: the question is hidden for this opportunity. No model call. */
export function useSkipQuestion(id: string) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (question: string): Promise<Brief> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/brief/questions/{question_id}/skip", {
          params: { path: { id, question_id: question } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** The problems the Prepare page has a plain message for (`prepare.errors.<code>`). */
const ERROR_CODES = [
  "site_needed",
  "invalid_site",
  "site_not_allowed",
  "site_unreachable",
  "robots_disallows",
  "no_pages_readable",
  "company_reads_daily_cap",
  "company_unknown",
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
  "invalid_answer",
  "question_closed",
  "profile_needed",
  "no_skill",
  "not_found",
] as const;

export type PrepareErrorCode = (typeof ERROR_CODES)[number] | "generic";

/** The problem's code if the messages know it, else `generic`. */
export function prepareErrorCode(error: unknown): PrepareErrorCode {
  const code = problemCode(error);
  return ERROR_CODES.find((known) => known === code) ?? "generic";
}

/** `https://example.com/blog/hotwire?x=1` as `example.com/blog/hotwire`. */
export function shownUrl(url: string): string {
  try {
    const parsed = new URL(url);
    const path = parsed.pathname === "/" ? "" : parsed.pathname.replace(/\/$/, "");
    return `${parsed.host.replace(/^www\./, "")}${path}`;
  } catch {
    return url;
  }
}

/** The stage from which a tailored CV can be prepared: Shortlisted or later. */
export function canPrepare(stage: Schema<"OpportunityStage">): boolean {
  return stage !== "found";
}

/**
 * About what preparing a job costs the first time, in euros: reading the
 * company (one call) and comparing the job (one call). The page itself shows
 * the server's own estimates.
 */
export const PREPARE_ESTIMATE_EUR = 0.004;
