import type { TFunction } from "i18next";

import { LABEL_HUES, type LabelHue } from "~/lib/ui";

import type { HiddenReason, JobFacts, OpportunityStage, OpportunitySummary } from "./opportunities";

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

/** The sources with a name of their own in every language (company boards, HN, a link). */
export const NAMED_SOURCES = ["greenhouse", "lever", "ashby", "arbeitnow", "hn", "link"] as const;
type NamedSource = (typeof NAMED_SOURCES)[number];

function isNamed(source: string): source is NamedSource {
  return (NAMED_SOURCES as readonly string[]).includes(source);
}

/** The named sources' colours. */
const NAMED_HUES: Record<NamedSource, LabelHue> = {
  greenhouse: "green",
  lever: "blue",
  ashby: "purple",
  arbeitnow: "pink",
  hn: "orange",
  link: "gray",
};

/** A source's colour: its own for a named one, else one picked from its slug (never gray). */
export function sourceHue(source: string): LabelHue {
  if (isNamed(source)) {
    return NAMED_HUES[source];
  }
  const hues = LABEL_HUES.filter((hue) => hue !== "gray");
  let sum = 0;
  for (const char of source) {
    sum = (sum * 31 + (char.codePointAt(0) ?? 0)) % 9973;
  }
  return hues[sum % hues.length] ?? "blue";
}

/** A source's name: the job board's as credited ("Himalayas"), else its own in the language. */
export function sourceLabel(
  row: Pick<OpportunitySummary, "source" | "source_name">,
  t: TFunction<"pipeline">,
): string {
  const name = row.source_name?.trim() ?? "";
  if (name !== "") {
    return name;
  }
  return isNamed(row.source) ? t(`sources.${row.source}`) : row.source;
}

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

/**
 * What an AI step costs, about, in euros: "€0.004" (a tenth of a cent is
 * shown; a few cents are shown as cents).
 */
export function formatCost(value: number, locale: string): string {
  const small = value > 0 && value < 0.01;
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "EUR",
    minimumFractionDigits: small ? 3 : 2,
    maximumFractionDigits: small ? 4 : 2,
  }).format(value);
}

/** A limit in euros: whole euros without cents ("€3"), else with them ("€0.30"). */
export function formatLimit(value: number, locale: string): string {
  const whole = Number.isInteger(value);
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "EUR",
    minimumFractionDigits: whole ? 0 : 2,
    maximumFractionDigits: 2,
  }).format(value);
}
