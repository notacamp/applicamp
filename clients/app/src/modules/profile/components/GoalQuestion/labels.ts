import { useTranslation } from "react-i18next";

import type {
  Autonomy,
  CashFloor,
  CompanyStage,
  Goals,
  Relocate,
  Seniority,
  WorkMode,
} from "../../api/goals";
import type { Question } from "../../api/questions";

/** `text` with its first letter in capitals (a language's name, in a language that writes them small). */
function capitalised(text: string, locale: string): string {
  const [first = ""] = text;
  return first.toLocaleUpperCase(locale) + text.slice(first.length);
}

/** The words for the goals: choices, languages, currencies, amounts, and each answer in a line. */
export function useGoalLabels() {
  const { t, i18n } = useTranslation("profile");
  const locale = i18n.language;

  const names = (type: "language" | "currency") => {
    try {
      return new Intl.DisplayNames(locale, { type });
    } catch {
      return null;
    }
  };
  const languageName = (tag: string): string => {
    const name = names("language")?.of(tag);
    return name === undefined ? tag : capitalised(name, locale);
  };
  const currencyName = (code: string): string => {
    const name = names("currency")?.of(code);
    return name === undefined ? code : `${code} · ${capitalised(name, locale)}`;
  };
  const money = (floor: CashFloor): string => {
    let amount: string;
    try {
      amount = new Intl.NumberFormat(locale, {
        style: "currency",
        currency: floor.currency,
        maximumFractionDigits: 0,
      }).format(floor.amount);
    } catch {
      amount = `${String(floor.amount)} ${floor.currency}`;
    }
    return `${amount} ${t(`goals.options.period.${floor.period}`)}`;
  };

  const option = {
    seniority: (value: Seniority) => t(`goals.options.seniority.${value}`),
    work_mode: (value: WorkMode) => t(`goals.options.work_mode.${value}`),
    relocate: (value: Relocate) => t(`goals.options.relocate.${value}`),
    company_stage: (value: CompanyStage) => t(`goals.options.company_stage.${value}`),
    autonomy: (value: Autonomy) => t(`goals.options.autonomy.${value}.title`),
  };

  /** One answer as a line (the Goals section's value); nothing when it is not answered. */
  const summary = (answers: Goals, question: Question): string | undefined => {
    switch (question) {
      case "roles":
      case "locations":
      case "avoid":
      case "must_have": {
        const items = answers[question];
        return items.length === 0 ? undefined : items.join(", ");
      }
      case "seniority": {
        return answers.seniority.length === 0
          ? undefined
          : answers.seniority.map((value) => option.seniority(value)).join(", ");
      }
      case "work_mode": {
        return answers.work_mode.length === 0
          ? undefined
          : answers.work_mode.map((value) => option.work_mode(value)).join(", ");
      }
      case "company_stage": {
        return answers.company_stage.length === 0
          ? undefined
          : answers.company_stage.map((value) => option.company_stage(value)).join(", ");
      }
      case "languages_for_work": {
        return answers.languages_for_work.length === 0
          ? undefined
          : answers.languages_for_work.map((tag) => languageName(tag)).join(", ");
      }
      case "relocate": {
        return answers.relocate == null ? undefined : option.relocate(answers.relocate);
      }
      case "autonomy": {
        return answers.autonomy == null ? undefined : option.autonomy(answers.autonomy);
      }
      case "cash_floor": {
        return answers.cash_floor == null ? undefined : money(answers.cash_floor);
      }
      case "equity_counts": {
        if (answers.equity_counts == null) {
          return undefined;
        }
        return t(`goals.options.equity.${answers.equity_counts ? "yes" : "no"}`);
      }
    }
  };

  return { languageName, currencyName, money, option, summary };
}
