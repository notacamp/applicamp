import { useTranslation } from "react-i18next";

import { Page, Placeholder } from "~/lib/ui";

/** `/agents`: the runs, each in its own sandbox, watched live. None yet. */
export function Agents() {
  const { t } = useTranslation();
  return (
    <Page testId="agents" title={t("agents.title")} lead={t("agents.lead")}>
      <Placeholder testId="agents-empty" icon="hourglass" title={t("agents.emptyTitle")}>
        {t("agents.emptyBody")}
      </Placeholder>
    </Page>
  );
}
