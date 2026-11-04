import { problemCode } from "~/lib/api";

/** The problems the page has a plain message for (`linkedin.errors.<code>`). */
const ERROR_CODES = [
  "invalid_upload",
  "file_too_large",
  "text_too_long",
  "not_a_zip",
  "archive_too_many_entries",
  "archive_entry_too_large",
  "archive_too_large",
  "archive_unsafe_path",
  "not_a_pdf",
  "pdf_unreadable",
  "pdf_too_many_pages",
  "pdf_no_text",
  "nothing_found",
  "profile_needed",
  "linkedin_daily_cap",
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
  "proposal_flagged",
  "proposal_not_kept",
  "invalid_text",
  "not_found",
] as const;

export type LinkedInErrorCode = (typeof ERROR_CODES)[number] | "generic";

/** The problem's code if the messages know it, else `generic`. */
export function errorCode(error: unknown): LinkedInErrorCode {
  const code = problemCode(error);
  return ERROR_CODES.find((known) => known === code) ?? "generic";
}
