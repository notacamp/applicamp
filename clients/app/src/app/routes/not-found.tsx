import { useTranslation } from "react-i18next";

import { AppBrand, ButtonLink, FocusPage, Tagged } from "~/lib/ui";

/** Any address the app does not know. */
export function NotFound() {
  const { t } = useTranslation();
  return (
    <Tagged testId="not-found">
      <FocusPage
        top={<AppBrand name={t("appName")} />}
        title={t("notFound.title")}
        lead={t("notFound.lead")}
        actions={
          <ButtonLink variant="primary" size="lg" href="/" data-testid="not-found-home">
            {t("notFound.home")}
          </ButtonLink>
        }
      />
    </Tagged>
  );
}
