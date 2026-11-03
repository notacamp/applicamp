// Invented companies, reasons and watches for the client's tests. No real
// company or person; the postings are made up.
import type { Schema } from "~/lib/api";

type CompanyList = Schema<"CompanyList">;
type CompanySuggestion = Schema<"CompanySuggestion">;
type CompanyReason = Schema<"CompanyReason">;
type WatchedCompany = Schema<"WatchedCompany">;

export const FABRIKAM = "0190a1b2-0000-7000-8000-0000000000c1";
export const NORTHWIND = "0190a1b2-0000-7000-8000-0000000000c2";
export const CONTOSO = "0190a1b2-0000-7000-8000-0000000000c3";

const POSTING_1 = "0190a1b2-0000-7000-8000-0000000000d1";
const POSTING_2 = "0190a1b2-0000-7000-8000-0000000000d2";
const POSTING_3 = "0190a1b2-0000-7000-8000-0000000000d3";

/** A reason with nothing to say yet; tests fill what they use. */
export function reason(overrides: Partial<CompanyReason> = {}): CompanyReason {
  return {
    kind: "matches",
    count: 3,
    total: 0,
    weeks: 6,
    values: [],
    sources: [],
    ...overrides,
  };
}

/** A company with three matches in six weeks, remote, stack and pay, each traced. */
export function suggestion(overrides: Partial<CompanySuggestion> = {}): CompanySuggestion {
  return {
    id: FABRIKAM,
    name: "Fabrikam Pay",
    website: "https://www.fabrikam.example/",
    reasons: [
      reason({
        sources: [
          { kind: "posting", id: POSTING_1, label: "Backend Engineer, Billing", score: 84 },
          { kind: "posting", id: POSTING_2, label: "Senior Rails Engineer", score: 81 },
          { kind: "posting", id: POSTING_3, label: "Staff Engineer", score: 72 },
        ],
      }),
      reason({
        kind: "stack",
        count: 3,
        total: 3,
        weeks: 0,
        values: ["Ruby on Rails", "PostgreSQL"],
        sources: [
          { kind: "posting", id: POSTING_1, label: "Backend Engineer, Billing", score: 84 },
        ],
      }),
      reason({
        kind: "remote",
        count: 3,
        total: 3,
        weeks: 0,
        sources: [{ kind: "posting", id: POSTING_2, label: "Senior Rails Engineer", score: 81 }],
      }),
      reason({
        kind: "pay",
        count: 0,
        weeks: 0,
        values: ["EUR 80000–105000 per year"],
        sources: [{ kind: "posting", id: POSTING_3, label: "Staff Engineer", score: 72 }],
      }),
    ],
    ...overrides,
  };
}

/** A company watched, its Greenhouse board read tonight. */
export function watching(overrides: Partial<WatchedCompany> = {}): WatchedCompany {
  return {
    id: NORTHWIND,
    name: "Northwind Stays",
    website: "https://northwind.example/",
    careers_url: null,
    board: "greenhouse",
    finding: false,
    needs_board: false,
    open_jobs: 23,
    watching_since: "2026-10-20T10:00:00Z",
    last_checked_at: "2026-10-26T21:00:00Z",
    last_error: null,
    ...overrides,
  };
}

/** The companies, with the limit the server sets. */
export function companies(suggested: CompanySuggestion[], watched: WatchedCompany[]): CompanyList {
  return { suggested, watching: watched, limit: 20 };
}
