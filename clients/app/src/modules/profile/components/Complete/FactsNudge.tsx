import { useTranslation } from "react-i18next";

import { Banner, Button, Tagged } from "~/lib/ui";

import { useFactProposals } from "../../api/facts";

export interface FactsNudgeProps {
  /** Open "Complete your profile". */
  onOpen: () => void;
}

/**
 * On Profile & goals, above the strength: how many facts the app found that
 * the profile does not hold yet, and the way to them. Nothing when there are
 * none.
 */
export function FactsNudge({ onOpen }: FactsNudgeProps) {
  const { t } = useTranslation("profile");
  const proposals = useFactProposals();
  const open = proposals.data?.open ?? 0;
  if (open === 0) {
    return null;
  }
  return (
    <Tagged testId="profile-facts-nudge">
      <Banner
        icon="check"
        title={t("complete.nudgeTitle", { count: open })}
        action={
          <Button variant="primary" data-testid="profile-facts-open" onClick={onOpen}>
            {t("complete.nudgeOpen")}
          </Button>
        }
      >
        {t("complete.nudgeBody")}
      </Banner>
    </Tagged>
  );
}
