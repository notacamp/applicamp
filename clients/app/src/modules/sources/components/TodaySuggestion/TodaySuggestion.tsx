import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Button,
  CompanyActions,
  CompanyCard,
  CompanyStatus,
  ReasonList,
  ReasonRow,
  Tagged,
  Text,
} from "~/lib/ui";

import {
  companyErrorOf,
  reasonText,
  sourceLines,
  useCompanies,
  useCompanyAction,
} from "../../api/companies";
import { hostOf } from "../../api/format";

export interface TodaySuggestionProps {
  /** Go to the Companies page's Watching tab (the board is being looked for there). */
  onSeeWatching?: (() => void) | undefined;
}

/**
 * Today's compact suggestion: one company whose jobs keep fitting the person,
 * with Watch, Not interested and Why? (the traced reasons). Nothing while there
 * is none, or while the companies are loading or failed: Today is not held up
 * by it.
 */
export function TodaySuggestion({ onSeeWatching }: TodaySuggestionProps) {
  const { t } = useTranslation("sources");
  const companies = useCompanies();
  const action = useCompanyAction();
  const [why, setWhy] = useState(false);
  const [watched, setWatched] = useState<string | null>(null);
  const suggestion = companies.data?.suggested[0];
  if (watched !== null) {
    return (
      <Tagged testId="today-suggestion-watching">
        <CompanyStatus>
          {t("companies.today.watching", { name: watched })}{" "}
          {onSeeWatching === undefined ? null : (
            <Button variant="link" onClick={onSeeWatching} data-testid="today-suggestion-see">
              {t("companies.today.see")}
            </Button>
          )}
        </CompanyStatus>
      </Tagged>
    );
  }
  if (suggestion === undefined) {
    return null;
  }
  const id = suggestion.id;
  const first = suggestion.reasons[0];
  const failed = action.isError ? companyErrorOf(action.error) : null;
  return (
    <CompanyCard
      title={t("companies.today.title", { name: suggestion.name })}
      aside={hostOf(suggestion.website)}
      testId="today-suggestion"
    >
      <Text>{t("companies.today.body", { name: suggestion.name })}</Text>
      {first === undefined ? null : (
        <Text tone="secondary" size="sm">
          {reasonText(first, t)}
        </Text>
      )}
      {why ? (
        <ReasonList
          label={t("companies.card.reasons", { name: suggestion.name })}
          testId="today-suggestion-reasons"
        >
          {suggestion.reasons.map((reason, index) => (
            <ReasonRow
              key={`${reason.kind}-${String(index)}`}
              text={reasonText(reason, t)}
              sources={sourceLines(reason, t)}
            />
          ))}
        </ReasonList>
      ) : null}
      {failed === null ? null : (
        <Tagged testId="today-suggestion-error">
          <Text tone="danger" size="sm">
            {t(`companies.errors.${failed}`, { max: "20" })}
          </Text>
        </Tagged>
      )}
      <CompanyActions>
        <Button
          variant="primary"
          disabled={action.isPending}
          onClick={() => {
            action.mutate(
              { kind: "watch", id },
              {
                onSuccess: () => {
                  setWatched(suggestion.name);
                },
              },
            );
          }}
          data-testid="today-suggestion-watch"
        >
          {t("companies.card.watch")}
        </Button>
        <Button
          variant="secondary"
          disabled={action.isPending}
          onClick={() => {
            action.mutate({ kind: "dismiss", id });
          }}
          data-testid="today-suggestion-dismiss"
        >
          {t("companies.card.notInterested")}
        </Button>
        <Button
          variant="link"
          aria-expanded={why}
          onClick={() => {
            setWhy((shown) => !shown);
          }}
          data-testid="today-suggestion-why"
        >
          {t("companies.card.why")}
        </Button>
      </CompanyActions>
    </CompanyCard>
  );
}
