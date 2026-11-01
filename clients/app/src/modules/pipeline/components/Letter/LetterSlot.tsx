import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Stack, Tagged, Text } from "~/lib/ui";

import { formatCost } from "../../api/labels";
import { letterBlockedReason, letterErrorCode, useLetter, useWriteLetter } from "../../api/letter";

/**
 * "Write the cover letter · about €0.002": one AI call, then the check without
 * AI. Off, with its reason, until the job's CV is approved (or when a cap or a
 * limit is hit). With a draft already made it says "Review the cover letter".
 */
export function LetterSlot({
  id,
  onReview,
  testIdPrefix = "letter",
}: {
  id: string;
  /** Open the review; none when the page is the review itself. */
  onReview?: (() => void) | undefined;
  testIdPrefix?: string | undefined;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const letter = useLetter(id);
  const write = useWriteLetter(id);
  if (letter.isPending) {
    return (
      <Tagged testId={`${testIdPrefix}-slot-loading`}>
        <Text tone="secondary">{t("letter.loading")}</Text>
      </Tagged>
    );
  }
  if (letter.isError) {
    return (
      <Tagged testId={`${testIdPrefix}-slot-error`}>
        <Banner tone="danger" icon="warning" title={t("letter.loadFailed")} />
      </Tagged>
    );
  }
  const data = letter.data;
  const draft = data.draft ?? undefined;
  const reason = letterBlockedReason(data);
  const cost = formatCost(data.draft_eur, i18n.language);
  const failure = write.isError ? letterErrorCode(write.error) : undefined;
  return (
    <Stack gap="sm">
      <Cluster>
        {draft === undefined ? (
          <Button
            variant="primary"
            ai
            disabled={reason !== undefined || write.isPending}
            data-testid={`${testIdPrefix}-write`}
            onClick={() => {
              write.reset();
              write.mutate(undefined, onReview === undefined ? {} : { onSuccess: onReview });
            }}
          >
            {write.isPending ? t("letter.slot.writing") : t("letter.slot.write", { cost })}
          </Button>
        ) : (
          <Button
            variant="primary"
            icon="arrow-right"
            data-testid={`${testIdPrefix}-review`}
            onClick={onReview}
          >
            {t("letter.slot.review")}
          </Button>
        )}
      </Cluster>
      {draft === undefined && reason !== undefined ? (
        <Tagged testId={`${testIdPrefix}-write-reason`}>
          <Text tone="secondary" size="sm">
            {t(`letter.slot.blocked.${reason}`)}
          </Text>
        </Tagged>
      ) : null}
      {draft === undefined && reason === undefined ? (
        <Text tone="secondary" size="sm">
          {t("letter.slot.hint")}
        </Text>
      ) : null}
      {failure === undefined ? null : (
        <Tagged testId={`${testIdPrefix}-write-failed`}>
          <Banner tone="danger" icon="warning" role="alert" title={t(`letter.errors.${failure}`)} />
        </Tagged>
      )}
    </Stack>
  );
}
