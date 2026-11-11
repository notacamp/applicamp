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

/** The gaps the server writes itself, in English (the model's own are shown as written). */
const SALARY_NOT_STATED = "Salary not stated";
const WHERE_UNCLEAR = "Doesn't say where it hires";
const HIRES_UNCLEAR = "Doesn't say if it hires in ";

/** Region codes by their English names ("Portugal" → "PT"), from the browser's own list. */
let regionCodes: Map<string, string> | null = null;

/** The code of the region named `name` in English; none when the browser doesn't know it. */
function regionCode(name: string): string | null {
  if (regionCodes === null) {
    regionCodes = new Map();
    try {
      const english = new Intl.DisplayNames(["en"], { type: "region", fallback: "none" });
      for (let first = 65; first <= 90; first += 1) {
        for (let second = 65; second <= 90; second += 1) {
          const code = String.fromCodePoint(first, second);
          const known = english.of(code);
          if (known !== undefined) {
            regionCodes.set(known, code);
          }
        }
      }
    } catch {
      // No region names in this browser: the English name is shown.
    }
  }
  return regionCodes.get(name) ?? null;
}

/**
 * A gap in the person's language when the server wrote it ("Doesn't say if
 * it hires in Portugal", "Salary not stated"), the country named by the
 * browser; any other gap as written.
 */
export function gapText(gap: string, locale: string, t: TFunction<"pipeline">): string {
  if (gap === SALARY_NOT_STATED) {
    return t("gaps.salaryNotStated");
  }
  if (gap === WHERE_UNCLEAR) {
    return t("gaps.whereUnclear");
  }
  if (gap.startsWith(HIRES_UNCLEAR)) {
    const name = gap.slice(HIRES_UNCLEAR.length);
    const code = regionCode(name);
    let country = name;
    if (code !== null) {
      try {
        country = new Intl.DisplayNames([locale], { type: "region" }).of(code) ?? name;
      } catch {
        // An unknown locale: the English name.
      }
    }
    return t("gaps.hiresUnclear", { country });
  }
  return gap;
}

/** Where it is, on one line: "Berlin, Germany · Hybrid"; empty when the posting says neither. */
export function placeText(
  row: Pick<OpportunitySummary, "location" | "work_mode">,
  t: TFunction<"pipeline">,
): string {
  const mode = row.work_mode ?? null;
  return [row.location?.trim() ?? "", mode === null ? "" : t(`workModes.${mode}`)]
    .filter((part) => part !== "")
    .join(" · ");
}

/**
 * What a job asks of where the person is, one item each: "Open to where you live: EU",
 * "Hours: CET ± 2 hours", "Work permit: no visa sponsorship", "Moving: to Berlin". None when
 * its posting was never checked.
 */
export function placeRequirements(
  facts: JobFacts | null | undefined,
  t: TFunction<"pipeline">,
): string[] {
  if (facts === null || facts === undefined) {
    return [];
  }
  const note = (text: string | null | undefined) => text?.trim() ?? "";
  const open = t(`facts.open.${facts.open_to_candidate}`);
  const region = note(facts.region_note);
  const items = [region === "" ? open : t("place.open", { open, note: region })];
  const notes = [
    ["place.timeZones", note(facts.time_zone_note)],
    ["place.workPermit", note(facts.work_permit_note)],
    ["place.relocation", note(facts.relocation_note)],
  ] as const;
  for (const [key, text] of notes) {
    if (text !== "") {
      items.push(t(key, { note: text }));
    }
  }
  return items;
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
