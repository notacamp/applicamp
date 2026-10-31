import { problemCode } from "~/lib/api";

/** The problems the wizard has a plain message for (`about.errors.<code>`). */
const ERROR_CODES = [
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
  "about_daily_cap",
  "profile_needed",
  "portrait_needed",
  "changes_flagged",
  "invalid_text",
  "invalid_site",
  "site_not_allowed",
  "invalid_change",
  "invalid_choice",
  "research_unavailable",
  "not_found",
] as const;

export type AboutErrorCode = (typeof ERROR_CODES)[number] | "generic";

/** The problem's code if the messages know it, else `generic`. */
export function errorCode(error: unknown): AboutErrorCode {
  const code = problemCode(error);
  return ERROR_CODES.find((known) => known === code) ?? "generic";
}

/** The first error among `errors` (a mutation that failed), if any. */
export function firstError(...errors: unknown[]): unknown {
  return errors.find((error) => error !== null && error !== undefined);
}
