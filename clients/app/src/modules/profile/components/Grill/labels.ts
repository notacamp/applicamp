import { problemCode } from "~/lib/api";

/** The problems Grill me has a plain message for (`grill.errors.<code>`). */
const ERROR_CODES = [
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
  "grill_daily_cap",
  "invalid_answer",
  "no_open_question",
  "grill_changed",
  "profile_changed",
  "fact_not_supported",
  "invalid_fact",
  "proposal_not_ready",
  "not_found",
] as const;

export type GrillErrorCode = (typeof ERROR_CODES)[number] | "generic";

/** The problem's code if the messages know it, else `generic`. */
export function errorCode(error: unknown): GrillErrorCode {
  const code = problemCode(error);
  return ERROR_CODES.find((known) => known === code) ?? "generic";
}

/** The first error among `errors` (a mutation that failed), if any. */
export function firstError(...errors: unknown[]): unknown {
  return errors.find((error) => error !== null && error !== undefined);
}
