import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";

import { api, CSRF_HEADERS, problemCode, unwrap, type Schema } from "~/lib/api";

/** The companies worth watching, and the ones watched. */
export type CompanyList = Schema<"CompanyList">;
/** A company worth watching, with the reasons, each traced to where it comes from. */
export type CompanySuggestion = Schema<"CompanySuggestion">;
/** A company whose own job board is read every day. */
export type WatchedCompany = Schema<"WatchedCompany">;
export type CompanyReason = Schema<"CompanyReason">;
export type CompanyReasonSource = Schema<"CompanyReasonSource">;
export type WatchBoardKind = Schema<"WatchBoardKind">;

export const COMPANIES_QUERY_KEY = ["companies"] as const;

/** How often the companies are asked again while a job board is being looked for. */
export const FINDING_POLL_MS = 1500;

/** Whether a job board is being looked for on one of the companies. */
export function isFinding(list: Pick<CompanyList, "watching"> | undefined): boolean {
  return list?.watching.some((company) => company.finding) ?? false;
}

/** The companies (`GET /api/v1/companies`), asked again while a board is being looked for. */
export const companiesQuery = queryOptions({
  queryKey: COMPANIES_QUERY_KEY,
  queryFn: async ({ signal }): Promise<CompanyList> =>
    unwrap(await api.GET("/api/v1/companies", { signal })),
  refetchInterval: (query) => (isFinding(query.state.data) ? FINDING_POLL_MS : false),
});

export function useCompanies() {
  return useQuery(companiesQuery);
}

/** What the person can do to a company. */
export type CompanyAction =
  | { kind: "watch"; id: string }
  | { kind: "dismiss"; id: string }
  | { kind: "stop"; id: string }
  | { kind: "careers"; id: string; url: string };

async function act(action: CompanyAction): Promise<CompanyList> {
  const path = { params: { path: { id: action.id } }, headers: CSRF_HEADERS };
  switch (action.kind) {
    case "watch": {
      return unwrap(await api.POST("/api/v1/companies/{id}/watch", path));
    }
    case "dismiss": {
      return unwrap(await api.POST("/api/v1/companies/{id}/not-interested", path));
    }
    case "stop": {
      return unwrap(await api.POST("/api/v1/companies/{id}/stop", path));
    }
    case "careers": {
      return unwrap(
        await api.POST("/api/v1/companies/{id}/careers", { ...path, body: { url: action.url } }),
      );
    }
  }
}

/**
 * Watch a suggestion, say "Not interested", stop watching, or give a watch a
 * careers page. The answer is the whole list, which replaces the cached one.
 */
export function useCompanyAction() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: act,
    onSuccess: (list) => {
      queryClient.setQueryData(COMPANIES_QUERY_KEY, list);
    },
  });
}

/** Watch a company yourself, from its careers page (or a link to its job board). */
export function useAddCompany() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (url: string): Promise<CompanyList> =>
      unwrap(await api.POST("/api/v1/companies", { body: { url }, headers: CSRF_HEADERS })),
    onSuccess: (list) => {
      queryClient.setQueryData(COMPANIES_QUERY_KEY, list);
    },
  });
}

/** Companies a person may watch at once (the server's limit, as the list says). */
export const COMPANY_ERRORS = ["watch_limit_reached", "invalid_url"] as const;
export type CompanyError = (typeof COMPANY_ERRORS)[number] | "generic";

/** What to say about an action that failed. */
export function companyErrorOf(error: unknown): CompanyError {
  const code = problemCode(error);
  return code !== undefined && (COMPANY_ERRORS as readonly string[]).includes(code)
    ? (code as CompanyError)
    : "generic";
}

/**
 * One reason as a sentence, in the person's language: worked out from the
 * posting fields the server cites, never rewritten.
 */
export function reasonText(reason: CompanyReason, t: TFunction<"sources">): string {
  const values = reason.values.join(", ");
  switch (reason.kind) {
    case "shortlisted": {
      return t("companies.reason.shortlisted", { count: reason.count });
    }
    case "matches": {
      return reason.weeks <= 1
        ? t("companies.reason.matchesWeek", { count: reason.count })
        : t("companies.reason.matches", { count: reason.count, weeks: String(reason.weeks) });
    }
    case "stack": {
      return t("companies.reason.stack", { skills: values });
    }
    case "remote": {
      return reason.count >= reason.total
        ? t("companies.reason.remoteAll")
        : t("companies.reason.remoteSome", {
            n: String(reason.count),
            total: String(reason.total),
          });
    }
    case "region": {
      return t("companies.reason.region", { places: values });
    }
    case "pay": {
      return t("companies.reason.pay", { pay: values });
    }
    case "fact": {
      return t("companies.reason.fact", { fact: values });
    }
  }
}

/** Where a reason comes from, one line per posting or fact. */
export function sourceLines(reason: CompanyReason, t: TFunction<"sources">): string[] {
  return reason.sources.map((source) => {
    if (source.kind === "fact") {
      return t("companies.source.fact", { company: source.label });
    }
    return source.score === undefined || source.score === null
      ? source.label
      : t("companies.source.posting", { title: source.label, score: String(source.score) });
  });
}

/**
 * The words for why a check failed, in the person's language: a code the server
 * sets (`http_503`, `unreachable`, `no_board`…); one the app does not know is
 * shown as it is.
 */
export function failureText(code: string, t: TFunction<"sources">): string {
  const known = [
    "no_board",
    "robots_disallows",
    "unreachable",
    "rate_limited",
    "unexpected_answer",
    "too_large",
    "refused_host",
  ] as const;
  const match = known.find((candidate) => candidate === code);
  if (match !== undefined) {
    return t(`companies.failure.${match}`);
  }
  const status = /^http_(\d{3})$/.exec(code)?.[1];
  return status === undefined ? code : t("companies.failure.http", { status });
}
