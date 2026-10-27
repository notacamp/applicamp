import type { TFunction } from "i18next";

import type { LabelHue } from "~/lib/ui";

import type { HiddenReason, JobFacts, OpportunitySource, OpportunityStage } from "./opportunities";

/** Each stage's colour, like a Notion select. */
export const STAGE_HUES: Record<OpportunityStage, LabelHue> = {
  found: "gray",
  shortlisted: "blue",
  applying: "purple",
  applied: "yellow",
  talking: "orange",
  interview: "pink",
  offer: "green",
  closed: "brown",
};

/** Each source's colour. */
export const SOURCE_HUES: Record<OpportunitySource, LabelHue> = {
  greenhouse: "green",
  lever: "blue",
  ashby: "purple",
  arbeitnow: "pink",
  hn: "orange",
  link: "gray",
};

/** A day, short: "26 Oct". */
export function formatShortDate(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { day: "numeric", month: "short" }).format(new Date(iso));
}

/** A language's name in `locale` ("en" → "English"); its code when the browser can't name it. */
export function languageName(code: string, locale: string): string {
  try {
    return new Intl.DisplayNames([locale], { type: "language" }).of(code) ?? code;
  } catch {
    return code;
  }
}

/**
 * Why it is hidden, with where it is open when that is the reason: "Not open to where you
 * live · Germany or Austria only".
 */
export function hiddenReasonText(
  reason: HiddenReason,
  regionNote: string | null | undefined,
  t: TFunction<"pipeline">,
): string {
  const note = regionNote?.trim() ?? "";
  const text = t(`hidden.reasons.${reason}`);
  return reason === "not_open_to_location" && note !== "" ? `${text} · ${note}` : text;
}

/** The facts on one line: "Remote · Open to where you live (EU) · English". */
export function factsText(facts: JobFacts, locale: string, t: TFunction<"pipeline">): string {
  const note = facts.region_note?.trim() ?? "";
  const open = t(`facts.open.${facts.open_to_candidate}`);
  const parts = [t(`facts.remote.${facts.remote}`), note === "" ? open : `${open} (${note})`];
  if (facts.working_languages.length > 0) {
    parts.push(facts.working_languages.map((code) => languageName(code, locale)).join(", "));
  }
  return parts.join(" · ");
}
