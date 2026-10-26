import { useTranslation } from "react-i18next";

import {
  CheckCards,
  ChoiceCards,
  Cluster,
  ListField,
  Segmented,
  Select,
  Stack,
  Tagged,
  TextField,
} from "~/lib/ui";

import type { Goals } from "../../api/goals";
import {
  AUTONOMIES,
  COMPANY_STAGES,
  CURRENCIES,
  isListQuestion,
  LIST_LIMITS,
  MAX_ITEM,
  PERIODS,
  RELOCATES,
  SENIORITIES,
  WORK_LANGUAGES,
  WORK_MODES,
  withItems,
  type Pending,
  type Question,
} from "../../api/questions";

import { useGoalLabels } from "./labels";

export interface GoalQuestionProps {
  question: Question;
  answers: Goals;
  /** The answers as the person changes them (nothing is saved here). */
  onAnswers: (answers: Goals) => void;
  /** What is typed and not yet part of the answers: a list's text, the cash floor's amount. */
  pending: Pending;
  onPending: (pending: Pending) => void;
  /** The cash floor's amount is not an amount. */
  amountInvalid?: boolean | undefined;
}

/** `value` with `extra` (answers outside the offered choices), once each. */
function withExtras(offered: readonly string[], extra: readonly string[]): string[] {
  return [...offered, ...extra.filter((value) => !offered.includes(value))];
}

function yesNo(value: boolean): "yes" | "no" {
  return value ? "yes" : "no";
}

/**
 * The controls of one goals question, the same in the interview and when an
 * answer is edited in place on Profile & goals. They only change what is
 * handed in; saving is the owner's.
 */
export function GoalQuestion({
  question,
  answers,
  onAnswers,
  pending,
  onPending,
  amountInvalid = false,
}: GoalQuestionProps) {
  const { t } = useTranslation("profile");
  const labels = useGoalLabels();
  const title = t(`goals.q.${question}.title`);
  const group = `goal-${question}`;

  if (isListQuestion(question)) {
    const max = LIST_LIMITS[question];
    return (
      <Tagged testId={`${group}-field`}>
        <ListField
          label={t(`goals.q.${question}.field`)}
          hint={t("goals.interview.limit", { max })}
          addHint={t("goals.interview.addAnother")}
          items={answers[question]}
          onItems={(items) => {
            onAnswers(withItems(answers, question, items));
          }}
          draft={pending.draft}
          onDraft={(draft) => {
            onPending({ ...pending, draft });
          }}
          max={max}
          maxLength={MAX_ITEM}
          placeholder={t(`goals.q.${question}.placeholder`)}
          addLabel={t("goals.interview.add")}
          removeLabel={(item) => t("goals.interview.remove", { item })}
          testId={group}
        />
      </Tagged>
    );
  }

  switch (question) {
    case "seniority": {
      return (
        <Tagged testId={`${group}-field`}>
          <CheckCards
            label={title}
            hideLabel
            name={group}
            options={SENIORITIES.map((value) => ({ value, title: labels.option.seniority(value) }))}
            value={answers.seniority}
            onChange={(seniority) => {
              onAnswers({ ...answers, seniority });
            }}
          />
        </Tagged>
      );
    }
    case "work_mode": {
      return (
        <Tagged testId={`${group}-field`}>
          <CheckCards
            label={title}
            hideLabel
            name={group}
            options={WORK_MODES.map((value) => ({ value, title: labels.option.work_mode(value) }))}
            value={answers.work_mode}
            onChange={(work_mode) => {
              onAnswers({ ...answers, work_mode });
            }}
          />
        </Tagged>
      );
    }
    case "company_stage": {
      return (
        <Tagged testId={`${group}-field`}>
          <CheckCards
            label={title}
            hideLabel
            name={group}
            options={COMPANY_STAGES.map((value) => ({
              value,
              title: labels.option.company_stage(value),
            }))}
            value={answers.company_stage}
            onChange={(company_stage) => {
              onAnswers({ ...answers, company_stage });
            }}
          />
        </Tagged>
      );
    }
    case "languages_for_work": {
      return (
        <Tagged testId={`${group}-field`}>
          <CheckCards
            label={title}
            hideLabel
            name={group}
            options={withExtras(WORK_LANGUAGES, answers.languages_for_work).map((value) => ({
              value,
              title: labels.languageName(value),
            }))}
            value={answers.languages_for_work}
            onChange={(languages_for_work) => {
              onAnswers({ ...answers, languages_for_work });
            }}
          />
        </Tagged>
      );
    }
    case "relocate": {
      return (
        <Tagged testId={`${group}-field`}>
          <ChoiceCards
            label={title}
            hideLabel
            name={group}
            options={RELOCATES.map((value) => ({ value, title: labels.option.relocate(value) }))}
            value={answers.relocate ?? null}
            onChange={(relocate) => {
              onAnswers({ ...answers, relocate });
            }}
          />
        </Tagged>
      );
    }
    case "autonomy": {
      return (
        <Tagged testId={`${group}-field`}>
          <ChoiceCards
            label={title}
            hideLabel
            name={group}
            options={AUTONOMIES.map((value) => ({
              value,
              title: labels.option.autonomy(value),
              description: t(`goals.options.autonomy.${value}.description`),
            }))}
            value={answers.autonomy ?? null}
            onChange={(autonomy) => {
              onAnswers({ ...answers, autonomy });
            }}
          />
        </Tagged>
      );
    }
    case "equity_counts": {
      const counts = answers.equity_counts;
      const chosen = counts === undefined || counts === null ? null : yesNo(counts);
      return (
        <Tagged testId={`${group}-field`}>
          <ChoiceCards
            label={title}
            hideLabel
            name={group}
            options={(["yes", "no"] as const).map((choice) => ({
              value: choice,
              title: t(`goals.options.equity.${choice}`),
            }))}
            value={chosen}
            onChange={(choice) => {
              onAnswers({ ...answers, equity_counts: choice === "yes" });
            }}
          />
        </Tagged>
      );
    }
    case "cash_floor": {
      return (
        <Tagged testId={`${group}-field`}>
          <Stack gap="md">
            <TextField
              label={t("goals.cash.amount")}
              size="lg"
              inputMode="numeric"
              autoComplete="off"
              value={pending.amount}
              placeholder={t("goals.cash.amountPlaceholder")}
              error={amountInvalid ? t("goals.interview.invalidAmount") : undefined}
              data-testid={`${group}-amount`}
              onChange={(event) => {
                onPending({ ...pending, amount: event.target.value });
              }}
            />
            <Cluster>
              <Select
                label={t("goals.cash.currency")}
                size="lg"
                value={pending.currency}
                options={withExtras(CURRENCIES, [pending.currency]).map((code) => ({
                  value: code,
                  label: labels.currencyName(code),
                }))}
                data-testid={`${group}-currency`}
                onChange={(currency) => {
                  onPending({ ...pending, currency });
                }}
              />
              <Segmented
                label={t("goals.cash.period")}
                options={PERIODS.map((value) => ({
                  value,
                  label: t(`goals.options.period.${value}`),
                }))}
                value={pending.period}
                onChange={(period) => {
                  onPending({ ...pending, period });
                }}
              />
            </Cluster>
          </Stack>
        </Tagged>
      );
    }
  }
}
