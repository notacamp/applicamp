import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Stack, Tagged, Text } from "~/lib/ui";

import { cvErrorCode, useCv, useMakeDraft, type Cv } from "../../api/cv";
import { formatCost } from "../../api/labels";

/** Why "Draft the CV" is off, in words (`cv.draft.blocked.<reason>`). */
export function blockedReason(cv: Cv): NonNullable<Cv["can_draft"]["reason"]> | undefined {
  return cv.can_draft.allowed ? undefined : (cv.can_draft.reason ?? undefined);
}

/**
 * "Draft the CV · about €0.002": one AI call, then the check without AI.
 * Off, with its reason, when the job is not yet shortlisted or a cap or a
 * limit is hit. With a draft already made it says "Review the CV".
 */
export function DraftSlot({
  id,
  onReview,
  testIdPrefix = "prepare",
}: {
  id: string;
  /** Open the review; none when the page is the review itself. */
  onReview?: (() => void) | undefined;
  testIdPrefix?: string | undefined;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const cv = useCv(id);
  const make = useMakeDraft(id);
  if (cv.isPending) {
    return (
      <Tagged testId={`${testIdPrefix}-draft-loading`}>
        <Text tone="secondary">{t("cv.loading")}</Text>
      </Tagged>
    );
  }
  if (cv.isError) {
    return (
      <Tagged testId={`${testIdPrefix}-draft-error`}>
        <Banner tone="danger" icon="warning" title={t("cv.loadFailed")} />
      </Tagged>
    );
  }
  const data = cv.data;
  const draft = data.draft ?? undefined;
  const reason = blockedReason(data);
  const cost = formatCost(data.draft_eur, i18n.language);
  const failure = make.isError ? cvErrorCode(make.error) : undefined;
  return (
    <Stack gap="sm">
      <Cluster>
        {draft === undefined ? (
          <Button
            variant="primary"
            ai
            disabled={reason !== undefined || make.isPending}
            data-testid={`${testIdPrefix}-draft`}
            onClick={() => {
              make.reset();
              make.mutate(undefined, onReview === undefined ? {} : { onSuccess: onReview });
            }}
          >
            {make.isPending ? t("cv.draft.drafting") : t("cv.draft.draft", { cost })}
          </Button>
        ) : (
          <Button
            variant="primary"
            icon="arrow-right"
            data-testid={`${testIdPrefix}-review`}
            onClick={onReview}
          >
            {t("cv.draft.review")}
          </Button>
        )}
      </Cluster>
      {draft === undefined && reason !== undefined ? (
        <Tagged testId={`${testIdPrefix}-draft-reason`}>
          <Text tone="secondary" size="sm">
            {t(`cv.draft.blocked.${reason}`)}
          </Text>
        </Tagged>
      ) : null}
      {draft === undefined && reason === undefined ? (
        <Text tone="secondary" size="sm">
          {t("cv.draft.hint")}
        </Text>
      ) : null}
      {failure === undefined ? null : (
        <Tagged testId={`${testIdPrefix}-draft-failed`}>
          <Banner tone="danger" icon="warning" role="alert" title={t(`cv.errors.${failure}`)} />
        </Tagged>
      )}
    </Stack>
  );
}
