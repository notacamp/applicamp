import { useTranslation } from "react-i18next";

import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { Banner, Button, Cluster, Tagged, Text } from "~/lib/ui";

import { FACT_PROPOSALS_QUERY_KEY, useFactProposals } from "../../api/facts";

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

/**
 * After Tell me about you is saved: one line saying how many facts wait on
 * Complete your profile (Save proposes what it did not write itself), and the
 * way there. Nothing when none wait.
 */
export function FactsWaiting({ onOpen }: FactsNudgeProps) {
  const { t } = useTranslation("profile");
  const queryClient = useQueryClient();
  // Save has just made them: read them again.
  useEffect(() => {
    void queryClient.invalidateQueries({ queryKey: FACT_PROPOSALS_QUERY_KEY });
  }, [queryClient]);
  const proposals = useFactProposals();
  const open = proposals.data?.open ?? 0;
  if (open === 0) {
    return null;
  }
  return (
    <Tagged testId="about-saved-facts">
      <Cluster>
        <Text>{t("about.saved.facts", { count: open })}</Text>
        <Button variant="secondary" data-testid="about-saved-facts-open" onClick={onOpen}>
          {t("complete.nudgeOpen")}
        </Button>
      </Cluster>
    </Tagged>
  );
}
