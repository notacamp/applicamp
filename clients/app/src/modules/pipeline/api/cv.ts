import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, problemCode, problemDetail, unwrap, type Schema } from "~/lib/api";

import { opportunityKey, PIPELINE_QUERY_KEY } from "./opportunities";

/** The tailored CV of an opportunity (`GET /api/v1/opportunities/{id}/cv`). */
export type Cv = Schema<"Cv">;
export type CvDraft = Schema<"CvDraftView">;
export type CvLine = Schema<"LineView">;
export type CvRole = Schema<"RoleView">;
export type CvSkill = Schema<"SkillView">;
export type CvReason = Schema<"Reason">;
export type CvMark = Schema<"Mark">;
export type CvKeyword = Schema<"Keyword">;
export type CvLeftOut = Schema<"LeftOut">;
export type CvSource = Schema<"SourceView">;
export type CvVersion = Schema<"CvVersion">;
export type CvPdfChecks = Schema<"PdfChecks">;
export type CvTemplate = Schema<"Template">;

/** The templates, as the person picks them. */
export const TEMPLATES = ["clean", "compact"] as const satisfies readonly CvTemplate[];
/** How many pages the PDF may take. */
export const LENGTHS = [1, 2] as const;
export type CvLength = (typeof LENGTHS)[number];

/** How a PDF looks: its template and its most pages. */
export interface CvLayout {
  template: CvTemplate;
  length: CvLength;
}

/** What a PDF is made with unless the person says otherwise. */
export const DEFAULT_LAYOUT: CvLayout = { template: "clean", length: 1 };

/** The layout of a version. */
export function layoutOf(version: Pick<CvVersion, "template" | "length">): CvLayout {
  return { template: version.template, length: version.length === 2 ? 2 : 1 };
}

/**
 * Where a version's PDF is: opened in the browser, or (with `download`) saved
 * under its own name, which the server keeps as the person's name and the company.
 */
export function cvPdfUrl(opportunity: string, version: number, download = false): string {
  const base = `/api/v1/opportunities/${encodeURIComponent(opportunity)}/cv/versions/${String(version)}/pdf`;
  return download ? `${base}?download=1` : base;
}

const cvKey = (id: string) => [...PIPELINE_QUERY_KEY, "cv", id] as const;
/** Every CV of the account (`GET /api/v1/cvs`). */
export const CVS_QUERY_KEY = [...PIPELINE_QUERY_KEY, "cvs"] as const;

/** Reading the CV never calls the model. */
export function useCv(id: string) {
  return useQuery({
    queryKey: cvKey(id),
    queryFn: async ({ signal }): Promise<Cv> =>
      unwrap(await api.GET("/api/v1/opportunities/{id}/cv", { params: { path: { id } }, signal })),
  });
}

/** Every change answers with the CV as it is now. */
function useStore(id: string) {
  const queryClient = useQueryClient();
  return (cv: Cv) => {
    queryClient.setQueryData(cvKey(id), cv);
    void queryClient.invalidateQueries({ queryKey: opportunityKey(id) });
    void queryClient.invalidateQueries({ queryKey: CVS_QUERY_KEY });
  };
}

/** Drafts a CV: one model call, then the check (no model). */
export function useMakeDraft(id: string, language?: string) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (): Promise<Cv> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/cv/drafts", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: language === undefined ? {} : { language },
        }),
      ),
    onSuccess: store,
  });
}

/** Edits a line (checked again against the same sources), or removes it. */
export function useChangeLine(id: string, n: number) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async ({
      line,
      change,
    }: {
      line: string;
      change: { text: string } | { remove: true };
    }): Promise<Cv> =>
      unwrap(
        await api.PATCH("/api/v1/opportunities/{id}/cv/drafts/{n}/lines/{line_id}", {
          params: { path: { id, n, line_id: line } },
          headers: CSRF_HEADERS,
          body: change,
        }),
      ),
    onSuccess: store,
  });
}

/** "It's true: add it to my profile": the person's words become a fact. */
export function useConfirmLine(id: string, n: number) {
  const store = useStore(id);
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ line, text }: { line: string; text?: string }): Promise<Cv> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/cv/drafts/{n}/lines/{line_id}/confirm", {
          params: { path: { id, n, line_id: line } },
          headers: CSRF_HEADERS,
          body: text === undefined ? {} : { text },
        }),
      ),
    onSuccess: (cv) => {
      store(cv);
      // The profile gained a fact or a skill (the profile module's keys).
      void queryClient.invalidateQueries({ queryKey: ["profile"] });
    },
  });
}

/** Puts back a position, or a line, as the profile has it. */
export function usePutBack(id: string, n: number) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (what: { position: number } | { source: string }): Promise<Cv> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/cv/drafts/{n}/put-back", {
          params: { path: { id, n } },
          headers: CSRF_HEADERS,
          body: what,
        }),
      ),
    onSuccess: store,
  });
}

/**
 * Approves a draft: its text becomes a PDF (Clean on 1 page unless the
 * `layout` says otherwise), read back and kept as the next version.
 */
export function useApprove(id: string, n: number) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (layout: CvLayout): Promise<Cv> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/cv/drafts/{n}/approve", {
          params: { path: { id, n } },
          headers: CSRF_HEADERS,
          body: layout,
        }),
      ),
    onSuccess: store,
  });
}

/** Another version of the approved CV in a new layout: no AI, the old versions stay. */
export function useMakeVersion(id: string) {
  const store = useStore(id);
  return useMutation({
    mutationFn: async (layout: CvLayout): Promise<Cv> =>
      unwrap(
        await api.POST("/api/v1/opportunities/{id}/cv/versions", {
          params: { path: { id } },
          headers: CSRF_HEADERS,
          body: layout,
        }),
      ),
    onSuccess: store,
  });
}

/** Every PDF version and every draft being reviewed, newest first. */
export type CvList = Schema<"CvList">;
export type CvListItem = Schema<"CvListItem">;

export function useCvs() {
  return useQuery({
    queryKey: CVS_QUERY_KEY,
    queryFn: async ({ signal }): Promise<CvList> =>
      unwrap(await api.GET("/api/v1/cvs", { signal })),
  });
}

/** The reasons a PDF was refused, one per line of the server's answer (in English). */
export function pdfFailureReasons(error: unknown): string[] {
  if (problemCode(error) !== "cv_pdf_check_failed") {
    return [];
  }
  return (problemDetail(error) ?? "").split("\n").filter((line) => line.trim() !== "");
}

/** The problems the CV pages have a plain message for (`cv.errors.<code>`). */
const ERROR_CODES = [
  "stage_too_early",
  "profile_needed",
  "cv_drafts_per_opportunity_cap",
  "cv_drafts_daily_cap",
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
  "invalid_text",
  "invalid_fact",
  "profile_changed",
  "draft_frozen",
  "lines_flagged",
  "already_on_cv",
  "cv_doesnt_fit",
  "cv_pdf_check_failed",
  "cv_pdf_unavailable",
  "cv_versions_cap",
  "not_approved",
  "same_layout",
  "invalid_length",
  "invalid_layout",
  "not_found",
] as const;

export type CvErrorCode = (typeof ERROR_CODES)[number] | "generic";

/** The problem's code if the messages know it, else `generic`. */
export function cvErrorCode(error: unknown): CvErrorCode {
  const code = problemCode(error);
  return ERROR_CODES.find((known) => known === code) ?? "generic";
}

/** What "Draft the CV" costs, about, in euros (the server's own figure is `Cv.draft_eur`). */
export const DRAFT_ESTIMATE_EUR = 0.002;

/** Where a flag can be fixed with "It's true": not for a company fact used as an achievement. */
export function canConfirm(flags: readonly CvReason[]): boolean {
  return (
    flags.length > 0 &&
    flags.every((flag) => flag.kind !== "company_fact_outside_summary" && flag.kind !== "no_source")
  );
}
