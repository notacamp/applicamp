import { useTranslation } from "react-i18next";

import { Fact, FactList, Page, Tagged } from "~/lib/ui";
import { PROVIDER_NAMES, useMe } from "~/modules/identity";
import { ProfileWorkspace, type Question } from "~/modules/profile";
import { SpendLine } from "~/modules/sources";

export interface ProfileProps {
  /** The import panel is asked for (`?import=new`). */
  importing: boolean;
  onImporting: (open: boolean) => void;
  /** Open the goals interview at a question. */
  onStartGoals: (question: Question) => void;
}

/** `/profile`: what agents know about you, how you sign in, and what the AI cost this month. */
export function Profile({ importing, onImporting, onStartGoals }: ProfileProps) {
  const { t } = useTranslation();
  const me = useMe();
  return (
    <Page testId="profile" title={t("profile.title")} lead={t("profile.lead")}>
      <ProfileWorkspace
        importing={importing}
        onImporting={onImporting}
        onStartGoals={onStartGoals}
      />
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
      <SpendLine />
    </Page>
  );
}
