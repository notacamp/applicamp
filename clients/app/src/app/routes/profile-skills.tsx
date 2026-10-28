import { useTranslation } from "react-i18next";

import { Page } from "~/lib/ui";
import { SkillsPanel } from "~/modules/profile";

export interface ProfileSkillsProps {
  /** Back to Profile & goals. */
  onBack: () => void;
}

/** `/profile/skills`: the skills in the order they count, and the ones found in your roles. */
export function ProfileSkills({ onBack }: ProfileSkillsProps) {
  const { t } = useTranslation();
  return (
    <Page
      testId="profile-skills"
      eyebrow={t("profileSkills.eyebrow")}
      title={t("profileSkills.title")}
      lead={t("profileSkills.lead")}
    >
      <SkillsPanel onBack={onBack} />
    </Page>
  );
}
