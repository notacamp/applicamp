import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, problemCode, unwrap, type Schema } from "~/lib/api";

import { CVS_QUERY_KEY } from "./cv";
import { opportunityKey, PIPELINE_QUERY_KEY } from "./opportunities";

/** The cover letter of an opportunity (`GET /api/v1/opportunities/{id}/letter`). */
export type Letter = Schema<"Letter">;
export type LetterDraft = Schema<"LetterDraftView">;
export type LetterParagraph = Schema<"LetterParagraphView">;
export type LetterSentence = Schema<"LetterSentenceView">;
export type LetterAnswer = Schema<"LetterAnswerView">;
export type LetterReason = Schema<"LetterReason">;
export type LetterTag = Schema<"LetterTag">;
export type LetterVersion = Schema<"LetterVersion">;
export type LetterSize = Schema<"LetterSize">;
export type LetterBlocked = Schema<"LetterBlocked">;

/** The lengths, as the person picks them (about 180 words, about 300). */
export const SIZES = ["short", "standard"] as const satisfies readonly LetterSize[];

/**
 * Where a version's PDF is: opened in the browser, or (with `download`) saved
 * under its own name, which the server keeps as the person's name and the company.
 */
export function letterPdfUrl(opportunity: string, version: number, download = false): string {
  const base = `/api/v1/opportunities/${encodeURIComponent(opportunity)}/letter/versions/${String(version)}/pdf`;
  return download ? `${base}?download=1` : base;
}

const letterKey = (id: string) => [...PIPELINE_QUERY_KEY, "letter", id] as const;

/** Reading the letter never calls the model. */
export function useLetter(id: string) {
  return useQuery({
    queryKey: letterKey(id),
    queryFn: async ({ signal }): Promise<Letter> =>
      unwrap(
        await api.GET("/api/v1/opportunities/{id}/letter", { params: { path: { id } }, signal }),
      ),
  });
}

/** Every change answers with the letter as it is now. */
function useStore(id: string) {
  const queryClient = useQueryClient();
  return (letter: Letter) => {
    queryClient.setQueryData(letterKey(id), letter);
    void queryClient.invalidateQueries({ queryKey: opportunityKey(id) });
    void queryClient.invalidateQueries({ queryKey: CVS_QUERY_KEY });
  };
}

/** Writes the letter: one model call, then the check (no model). */
export function useWriteLetter(id: string, options: { language?: string; size?: LetterSize } = {}) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (): Promise<Letter> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/letter/drafts", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: {
            ...(options.language === undefined ? {} : { language: options.language }),
            ...(options.size === undefined ? {} : { size: options.size }),
          },
        }),
      ),
    onSuccess: store,
  });
}

/** Edits a sentence (checked again against the same sources), or removes it. */
export function useChangeSentence(id: string, n: number) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async ({
      sentence,
      change,
    }: {
      sentence: string;
      change: { text: string } | { remove: true };
    }): Promise<Letter> =>
      unwrap(
        await api.PATCH("/api/v1/opportunities/{id}/letter/drafts/{n}/sentences/{sentence_id}", {
          params: { path: { id, n, sentence_id: sentence } },
          headers: CSRF_HEADERS,
          body: change,
        }),
      ),
    onSuccess: store,
  });
}

/** "It's true: add it to my profile": the person's words become a fact. */
export function useConfirmSentence(id: string, n: number) {
  const store = useStore(id);
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ sentence, text }: { sentence: string; text: string }): Promise<Letter> =>
      unwrap(
        await api.POST(
          "/api/v1/opportunities/{id}/letter/drafts/{n}/sentences/{sentence_id}/confirm",
          {
            params: { path: { id, n, sentence_id: sentence } },
            headers: CSRF_HEADERS,
            body: { text },
          },
        ),
      ),
    onSuccess: (letter) => {
      store(letter);
      // The profile gained a fact (the profile module's keys).
      void queryClient.invalidateQueries({ queryKey: ["profile"] });
    },
  });
}

/** The person's own answer to one of the job's questions, or leaving it out. */
export function useChangeAnswer(id: string, n: number) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async ({
      answer,
      change,
    }: {
      answer: string;
      change: { text: string } | { remove: true };
    }): Promise<Letter> =>
      unwrap(
        await api.PATCH("/api/v1/opportunities/{id}/letter/drafts/{n}/answers/{answer_id}", {
          params: { path: { id, n, answer_id: answer } },
          headers: CSRF_HEADERS,
          body: change,
        }),
      ),
    onSuccess: store,
  });
}

/** Approves a draft: its text becomes a one-page PDF, read back and kept as the next version. */
export function useApproveLetter(id: string, n: number) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (): Promise<Letter> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/letter/drafts/{n}/approve", {
          params: { path: { id, n } },
          headers: CSRF_HEADERS,
        }),
      ),
    onSuccess: store,
  });
}

/** The problems the letter pages have a plain message for (`letter.errors.<code>`). */
const ERROR_CODES = [
  "stage_too_early",
  "no_approved_cv",
  "profile_needed",
  "letter_drafts_per_opportunity_cap",
  "letter_drafts_daily_cap",
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
  "invalid_text",
  "invalid_fact",
  "profile_changed",
  "draft_frozen",
  "letter_flagged",
  "letter_empty",
  "letter_doesnt_fit",
  "letter_pdf_check_failed",
  "letter_pdf_unavailable",
  "not_found",
] as const;

export type LetterErrorCode = (typeof ERROR_CODES)[number] | "generic";

/** The problem's code if the messages know it, else `generic`. */
export function letterErrorCode(error: unknown): LetterErrorCode {
  const code = problemCode(error);
  return ERROR_CODES.find((known) => known === code) ?? "generic";
}

/** What "Write the cover letter" costs, about, in euros (the server's figure is `Letter.draft_eur`). */
export const LETTER_ESTIMATE_EUR = 0.002;

/** Whether a letter would do better to be asked for than to be offered (the CV is approved). */
export function letterBlockedReason(letter: Letter): LetterBlocked | undefined {
  return letter.can_write.allowed ? undefined : (letter.can_write.reason ?? undefined);
}
