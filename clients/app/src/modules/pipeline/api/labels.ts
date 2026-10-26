import type { LabelHue } from "~/lib/ui";

import type { OpportunitySource, OpportunityStage } from "./opportunities";

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
