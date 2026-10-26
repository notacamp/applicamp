import { useTranslation } from "react-i18next";

import { AppBrand, ButtonLink, FocusPage, Tagged } from "~/lib/ui";

/**
 * `/not-invited`: the server sends a browser here when the address it signed
 * in with is not on the invite list. No account was created and no session
 * started; the way back is the sign-in page.
 */
export function NotInvitedPage() {
  const { t } = useTranslation("identity");
  const { t: tShell } = useTranslation();
  return (
    <Tagged testId="not-invited">
      <FocusPage
        top={<AppBrand name={tShell("appName")} />}
        title={t("notInvited.title")}
        lead={t("notInvited.lead")}
        footer={t("notInvited.footer")}
        actions={
          <ButtonLink variant="primary" size="lg" href="/sign-in" data-testid="back-to-sign-in">
            {t("notInvited.back")}
          </ButtonLink>
        }
      />
    </Tagged>
  );
}
