import { useTranslation } from "react-i18next";

import { Button, Section, Tagged, Text } from "~/lib/ui";

import { useLinkedIn } from "../../api/linkedin";

export interface LinkedInSectionProps {
  /** Open "Your LinkedIn". */
  onOpen: () => void;
}

/**
 * The way into "Your LinkedIn" from Profile & goals, with how many things
 * wait to be changed on LinkedIn when a review exists. Showing it asks
 * nothing of the model.
 */
export function LinkedInSection({ onOpen }: LinkedInSectionProps) {
  const { t } = useTranslation("profile");
  const linkedin = useLinkedIn();
  const review = linkedin.data?.review ?? null;
  return (
    <Section
      title={t("linkedin.sectionTitle")}
      description={t("linkedin.sectionLead")}
      action={
        <Button variant="secondary" icon="arrow-right" data-testid="linkedin-open" onClick={onOpen}>
          {review === null ? t("linkedin.sectionOpen") : t("linkedin.sectionList")}
        </Button>
      }
      testId="linkedin-section"
      anchor="linkedin"
    >
      {review === null ? null : (
        <Tagged testId="linkedin-section-counts">
          <Text tone="secondary" size="sm">
            {t("linkedin.sectionCounts", {
              open: review.open,
              kept: review.kept,
              done: review.done,
            })}
          </Text>
        </Tagged>
      )}
    </Section>
  );
}
