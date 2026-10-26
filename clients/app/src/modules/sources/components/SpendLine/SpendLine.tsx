import { useTranslation } from "react-i18next";

import { Tagged, Text } from "~/lib/ui";

import { formatEuros, formatLimit, useEstimate, type Spend } from "../../api/searches";

/** "AI this month: €0.42 of €3", in the person's language. */
export function spendText(
  spend: Spend,
  language: string,
  t: (key: "spend.month" | "spend.monthNoLimit", values: Record<string, string>) => string,
): string {
  const spent = formatEuros(spend.month_eur, language);
  const limit = spend.month_limit_eur;
  return limit === null || limit === undefined
    ? t("spend.monthNoLimit", { spent })
    : t("spend.month", { spent, limit: formatLimit(limit, language) });
}

/**
 * What the account spent on AI this month, against its limit. Nothing while
 * it loads or when it cannot be read.
 */
export function SpendLine({ testId = "spend-line" }: { testId?: string | undefined }) {
  const { t, i18n } = useTranslation("sources");
  const estimate = useEstimate();
  if (estimate.data === undefined) {
    return null;
  }
  return (
    <Tagged testId={testId}>
      <Text tone="secondary" size="sm">
        {spendText(estimate.data.spend, i18n.language, (key, values) =>
          key === "spend.month" ? t("spend.month", values) : t("spend.monthNoLimit", values),
        )}
      </Text>
    </Tagged>
  );
}
