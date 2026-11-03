import { useTranslation } from "react-i18next";

import { Page } from "~/lib/ui";
import { CompaniesPage, type CompaniesTab } from "~/modules/sources";

export interface CompaniesProps {
  tab: CompaniesTab;
  onTab: (tab: CompaniesTab) => void;
}

/**
 * `/companies`: the companies whose jobs keep fitting you (Suggested), the
 * ones whose own job board is read every day (`?tab=watching`), and how it
 * works and what it costs (`?tab=how`).
 */
export function Companies({ tab, onTab }: CompaniesProps) {
  const { t } = useTranslation();
  return (
    <Page testId="companies" title={t("companies.title")} lead={t("companies.lead")}>
      <CompaniesPage tab={tab} onTab={onTab} />
    </Page>
  );
}
