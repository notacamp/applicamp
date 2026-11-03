import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Button, CompanyActions, CompanyCard, ReasonList, ReasonRow, Tagged, Text } from "~/lib/ui";

import {
  companyErrorOf,
  reasonText,
  sourceLines,
  useCompanyAction,
  type CompanySuggestion,
} from "../../api/companies";
import { hostOf } from "../../api/format";

export interface SuggestionCardProps {
  suggestion: CompanySuggestion;
  /** The person said Watch: its job board is being looked for. */
  onWatching?: (() => void) | undefined;
}

/**
 * A company whose jobs keep fitting the person: why (each reason traced to the
 * postings it comes from, shown with Why?), and Watch or Not interested.
 */
export function SuggestionCard({ suggestion, onWatching }: SuggestionCardProps) {
  const { t } = useTranslation("sources");
  const action = useCompanyAction();
  const [why, setWhy] = useState(false);
  const id = suggestion.id;
  const failed = action.isError ? companyErrorOf(action.error) : null;
  return (
    <CompanyCard
      title={suggestion.name}
      aside={hostOf(suggestion.website)}
      testId={`company-card-${id}`}
    >
      <ReasonList
        label={t("companies.card.reasons", { name: suggestion.name })}
        testId={`company-reasons-${id}`}
      >
        {suggestion.reasons.map((reason, index) => (
          <ReasonRow
            key={`${reason.kind}-${String(index)}`}
            text={reasonText(reason, t)}
            sources={why ? sourceLines(reason, t) : undefined}
            testId={`company-reason-${id}-${reason.kind}`}
          />
        ))}
      </ReasonList>
      {failed === null ? null : (
        <Tagged testId={`company-error-${id}`}>
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
                  onWatching?.();
                },
              },
            );
          }}
          data-testid={`company-watch-${id}`}
        >
          {t("companies.card.watch")}
        </Button>
        <Button
          variant="secondary"
          disabled={action.isPending}
          onClick={() => {
            action.mutate({ kind: "dismiss", id });
          }}
          data-testid={`company-dismiss-${id}`}
        >
          {t("companies.card.notInterested")}
        </Button>
        <Button
          variant="link"
          aria-expanded={why}
          onClick={() => {
            setWhy((shown) => !shown);
          }}
          data-testid={`company-why-${id}`}
        >
          {t("companies.card.why")}
        </Button>
      </CompanyActions>
    </CompanyCard>
  );
}
