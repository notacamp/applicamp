import type { TFunction } from "i18next";

import type { Schema } from "~/lib/api";

import { shortDate } from "./format";

export type Waiting = Schema<"BoardWaiting">;

/** The reasons the page words; any other code gets the generic sentence. */
const KNOWN: ReadonlySet<string> = new Set<Schema<"BoardWaitingCode">>([
  "needs_key",
  "list_not_allowed",
  "no_job_list",
  "request_limit",
  "empty_list",
  "jobs_on_hiring_system",
  "not_a_job_board",
  "nothing_recent",
  "undated",
  "terms_forbid",
  "terms_ask_first",
  "terms_personal_only",
  "terms_unread",
  "owner_edited",
  "config_to_use",
  "needs_approval",
]);

/** The reasons that quote the sentence of the site's terms that decided them. */
const QUOTED: ReadonlySet<string> = new Set([
  "terms_forbid",
  "terms_ask_first",
  "terms_personal_only",
]);

/** Why a site still waits, in plain words in the person's language, with the detail filled in. */
export function whyText(waiting: Waiting, t: TFunction<"sources">, locale: string): string {
  const detail = waiting.detail ?? "";
  const key = KNOWN.has(waiting.code) ? waiting.code : "unknown";
  const dated = key === "nothing_recent" && !Number.isNaN(Date.parse(detail));
  const shown = dated ? shortDate(detail, locale) : detail;
  const sentence = t(`admin.why.${key}`, { detail: shown });
  return QUOTED.has(key) && detail !== "" ? `${sentence} \u201C${detail}\u201D` : sentence;
}
