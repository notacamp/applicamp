import { useTranslation } from "react-i18next";

import { Fact, FactList, Page, Placeholder, Tagged } from "~/lib/ui";
import { PROVIDER_NAMES, useMe } from "~/modules/identity";

/** `/profile`: what agents know about you. For now, only how you sign in. */
export function Profile() {
  const { t } = useTranslation();
  const me = useMe();
  return (
    <Page testId="profile" title={t("profile.title")} lead={t("profile.lead")}>
      {me.data === undefined ? null : (
        <Tagged testId="profile-account">
          <FactList>
            <Fact label={t("profile.signedInAs")}>{me.data.email}</Fact>
            <Fact label={t("profile.waysIn")}>
              {me.data.ways_in.map((provider) => PROVIDER_NAMES[provider]).join(", ")}
            </Fact>
          </FactList>
        </Tagged>
      )}
      <Placeholder testId="profile-empty" icon="people" title={t("profile.emptyTitle")}>
        {t("profile.emptyBody")}
      </Placeholder>
    </Page>
  );
}
