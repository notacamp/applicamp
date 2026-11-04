import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Page, Placeholder, Stack, Tagged, Text } from "~/lib/ui";

import { useLinkedIn } from "../../api/linkedin";
import { ReviewStep } from "./ReviewStep";
import { UploadStep } from "./UploadStep";

export interface LinkedInPageProps {
  /** Back to Profile & goals. */
  onBack: () => void;
}

/**
 * `/profile/linkedin`, "Your LinkedIn": the LinkedIn profile from a file the
 * person uploads, reviewed against their profile and goals, with what to
 * change on LinkedIn as proposals to keep or drop, and the list of kept
 * ones to copy over. Nothing is fetched from LinkedIn or written there.
 */
export function LinkedInPage({ onBack }: LinkedInPageProps) {
  const { t } = useTranslation("profile");
  const linkedin = useLinkedIn();
  /** Check again: the upload step over a review that exists. */
  const [again, setAgain] = useState(false);
  const view = linkedin.data;

  const body = () => {
    if (linkedin.isPending) {
      return (
        <Tagged testId="linkedin-loading">
          <Text tone="secondary">{t("linkedin.loading")}</Text>
        </Tagged>
      );
    }
    if (linkedin.isError || view === undefined) {
      return (
        <Tagged testId="linkedin-load-error">
          <Banner
            tone="danger"
            icon="warning"
            title={t("linkedin.loadFailed")}
            action={
              <Button
                variant="secondary"
                onClick={() => {
                  void linkedin.refetch();
                }}
              >
                {t("retry")}
              </Button>
            }
          />
        </Tagged>
      );
    }
    if (!view.has_profile) {
      return (
        <Placeholder
          testId="linkedin-no-profile"
          icon="people"
          title={t("linkedin.noProfileTitle")}
          action={
            <Button variant="primary" data-testid="linkedin-no-profile-back" onClick={onBack}>
              {t("linkedin.back")}
            </Button>
          }
        >
          {t("linkedin.noProfileLead")}
        </Placeholder>
      );
    }
    if (!view.ai_available) {
      return (
        <Tagged testId="linkedin-no-ai">
          <Banner icon="warning" title={t("linkedin.noAiTitle")}>
            {t("linkedin.noAiLead")}
          </Banner>
        </Tagged>
      );
    }
    const review = view.review ?? null;
    if (review === null || again) {
      return (
        <UploadStep
          view={view}
          onDone={() => {
            setAgain(false);
          }}
          onCancel={
            review === null
              ? undefined
              : () => {
                  setAgain(false);
                }
          }
        />
      );
    }
    return (
      <ReviewStep
        review={review}
        onAgain={() => {
          setAgain(true);
        }}
      />
    );
  };

  return (
    <Page testId="linkedin" title={t("linkedin.title")}>
      <Stack gap="lg">
        <Cluster>
          <Button variant="ghost" icon="chevron-left" data-testid="linkedin-back" onClick={onBack}>
            {t("linkedin.back")}
          </Button>
        </Cluster>
        {body()}
      </Stack>
    </Page>
  );
}
