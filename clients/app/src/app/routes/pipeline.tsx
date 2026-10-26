import { useTranslation } from "react-i18next";

import { Page, Placeholder } from "~/lib/ui";

/** `/pipeline`: every opportunity, from first match to offer. None yet. */
export function Pipeline() {
  const { t } = useTranslation();
  return (
    <Page testId="pipeline" title={t("pipeline.title")} lead={t("pipeline.lead")}>
      <Placeholder testId="pipeline-empty" icon="tasks" title={t("pipeline.emptyTitle")}>
        {t("pipeline.emptyBody")}
      </Placeholder>
    </Page>
  );
}
