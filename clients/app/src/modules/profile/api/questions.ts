import { withItem } from "~/lib/ui";

import type {
  Autonomy,
  CashFloor,
  CompanyStage,
  Goals,
  Period,
  Relocate,
  Seniority,
  WorkMode,
} from "./goals";

/** The interview's questions, in the order they are asked (also the keys of the answers). */
export const QUESTIONS = [
  "roles",
  "seniority",
  "work_mode",
  "locations",
  "relocate",
  "cash_floor",
  "equity_counts",
  "company_stage",
  "avoid",
  "must_have",
  "languages_for_work",
  "autonomy",
] as const satisfies readonly (keyof Goals)[];

export type Question = (typeof QUESTIONS)[number];

/** The choices of each question that picks among a few (the API's enums). */
export const SENIORITIES: readonly Seniority[] = [
  "junior",
  "mid",
  "senior",
  "staff",
  "principal",
  "lead",
  "manager",
  "director",
  "founding",
];
export const WORK_MODES: readonly WorkMode[] = ["remote", "hybrid", "onsite"];
export const RELOCATES: readonly Relocate[] = ["no", "within_country", "within_eu", "anywhere"];
export const COMPANY_STAGES: readonly CompanyStage[] = [
  "early",
  "growth",
  "established",
  "public",
  "nonprofit",
];
export const AUTONOMIES: readonly Autonomy[] = ["find_only", "prepare", "prepare_and_schedule"];
export const PERIODS: readonly Period[] = ["year", "month"];

/** The languages offered for work (BCP 47 tags); others already answered are kept. */
export const WORK_LANGUAGES = ["en", "pt", "es", "fr", "de", "it", "nl", "pl", "sv"] as const;

/** The currencies offered (ISO 4217); one already answered is kept. */
export const CURRENCIES = [
  "EUR",
  "USD",
  "GBP",
  "CHF",
  "CAD",
  "AUD",
  "NZD",
  "SEK",
  "NOK",
  "DKK",
  "PLN",
  "CZK",
  "HUF",
  "RON",
  "BGN",
  "TRY",
  "BRL",
  "MXN",
  "ARS",
  "CLP",
  "COP",
  "INR",
  "JPY",
  "CNY",
  "KRW",
  "SGD",
  "HKD",
  "AED",
  "SAR",
  "ILS",
  "ZAR",
  "NGN",
  "KES",
  "EGP",
  "MAD",
] as const;

/** Most items per list question, as the server holds them. */
export const LIST_LIMITS = {
  roles: 5,
  locations: 10,
  avoid: 10,
  must_have: 10,
} as const;
/** Longest item of a list, and the largest cash floor. */
export const MAX_ITEM = 100;
export const MAX_AMOUNT = 100_000_000;

export type ListQuestion = keyof typeof LIST_LIMITS;

export function isListQuestion(question: Question): question is ListQuestion {
  return question in LIST_LIMITS;
}

/** Whether `question` has an answer. */
export function isAnswered(answers: Goals, question: Question): boolean {
  const value = answers[question];
  return Array.isArray(value) ? value.length > 0 : value !== undefined && value !== null;
}

/** How many questions have an answer. */
export function answeredCount(answers: Goals): number {
  return QUESTIONS.filter((question) => isAnswered(answers, question)).length;
}

/**
 * The questions asked of someone with `answers`: all of them, except that
 * equity only matters once there is a cash floor.
 */
export function questionsFor(answers: Goals): Question[] {
  return QUESTIONS.filter(
    (question) => question !== "equity_counts" || isAnswered(answers, "cash_floor"),
  );
}

/** The first question without an answer, else the first. */
export function firstOpen(answers: Goals): Question {
  return questionsFor(answers).find((question) => !isAnswered(answers, question)) ?? QUESTIONS[0];
}

/** The question after `question` (or before it), if there is one. */
export function neighbour(
  answers: Goals,
  question: Question,
  direction: "next" | "previous",
): Question | undefined {
  const asked = questionsFor(answers);
  const at = QUESTIONS.indexOf(question);
  const candidates =
    direction === "next" ? QUESTIONS.slice(at + 1) : QUESTIONS.slice(0, at).toReversed();
  return candidates.find((candidate) => asked.includes(candidate));
}

/** What is typed in the question on screen and not yet part of the answers. */
export interface Pending {
  /** A list question's text, not yet added. */
  draft: string;
  /** The cash floor's amount, as typed. */
  amount: string;
  currency: string;
  period: Period;
}

/** The pending state of the question, from the answers (a cash floor fills it). */
export function pendingOf(answers: Goals): Pending {
  const floor = answers.cash_floor;
  return {
    draft: "",
    amount: floor === undefined || floor === null ? "" : String(floor.amount),
    currency: floor?.currency ?? "EUR",
    period: floor?.period ?? "year",
  };
}

/** The cash floor typed, `null` for none, `"invalid"` for something that is not an amount. */
export function floorOf(pending: Pending): CashFloor | null | "invalid" {
  const typed = pending.amount.trim();
  if (typed === "") {
    return null;
  }
  // "90000", or grouped by thousands: "90,000", "90.000", "90 000".
  const digits = /^\d{1,3}([ ,.']\d{3})+$/.test(typed) ? typed.replaceAll(/\D/g, "") : typed;
  const amount = Number(digits);
  if (!/^\d+$/.test(digits) || amount < 1 || amount > MAX_AMOUNT) {
    return "invalid";
  }
  return { amount, currency: pending.currency, period: pending.period };
}

/** `answers` with the items of a list question replaced. */
export function withItems(answers: Goals, question: ListQuestion, items: string[]): Goals {
  switch (question) {
    case "roles": {
      return { ...answers, roles: items };
    }
    case "locations": {
      return { ...answers, locations: items };
    }
    case "avoid": {
      return { ...answers, avoid: items };
    }
    case "must_have": {
      return { ...answers, must_have: items };
    }
  }
}

/** `answers` without the cash floor (not answered), or with it. */
export function withFloor(answers: Goals, floor: CashFloor | null): Goals {
  const next: Goals = { ...answers };
  if (floor === null) {
    delete next.cash_floor;
  } else {
    next.cash_floor = floor;
  }
  return next;
}

/** A value as text with its keys in order, nothing for a missing answer. */
function canonical(value: unknown): string {
  if (Array.isArray(value)) {
    return `[${value.map((item) => canonical(item)).join(",")}]`;
  }
  if (typeof value === "object" && value !== null) {
    const entries = Object.entries(value)
      .filter(([, item]) => item !== undefined && item !== null)
      .toSorted(([a], [b]) => a.localeCompare(b))
      .map(([key, item]) => `${JSON.stringify(key)}:${canonical(item)}`);
    return `{${entries.join(",")}}`;
  }
  return JSON.stringify(value);
}

/** Whether two sets of answers say the same. */
export function sameAnswers(a: Goals, b: Goals): boolean {
  return canonical(a) === canonical(b);
}

/**
 * `working` with what is typed in `question` made part of it: a list's text
 * added, the cash floor's amount read. `"invalid"` when the amount is not one.
 */
export function settle(question: Question, working: Goals, pending: Pending): Goals | "invalid" {
  if (isListQuestion(question)) {
    return withItems(
      working,
      question,
      withItem(working[question], pending.draft, LIST_LIMITS[question]),
    );
  }
  if (question === "cash_floor") {
    const floor = floorOf(pending);
    return floor === "invalid" ? "invalid" : withFloor(working, floor);
  }
  return working;
}
