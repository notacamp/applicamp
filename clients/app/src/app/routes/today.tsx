import { useTranslation } from "react-i18next";

import { formatDay } from "~/lib/i18n";
import { ButtonLink, Page, Placeholder } from "~/lib/ui";

import { useImportLink } from "../links";

/** `/`: what waits on you. On a first visit, nothing yet: set up the profile. */
export function Today() {
  const { t, i18n } = useTranslation();
  const importLink = useImportLink();
  return (
    <Page
      testId="today"
      eyebrow={formatDay(i18n.language)}
      title={t("today.title")}
      lead={t("today.lead")}
    >
      <Placeholder
        testId="today-empty"
        icon="check"
        title={t("today.emptyTitle")}
        action={
          <ButtonLink
            variant="primary"
            icon="arrow-right"
            data-testid="set-up-profile"
            {...importLink}
          >
            {t("today.setUp")}
          </ButtonLink>
        }
      >
        {t("today.emptyBody")}
      </Placeholder>
    </Page>
  );
}
