import { useTranslation } from "react-i18next";

import { SkeletonList, Stack, Tagged, Text } from "~/lib/ui";

/**
 * While the app writes the CV: what it is doing and when it is done, over a
 * quiet placeholder for the page (never a spinner in the content).
 */
export function WritingState({ company }: { company: string }) {
  const { t } = useTranslation("pipeline");
  return (
    <Tagged testId="cv-writing">
      <Stack>
        <Text>{t("cv.writing.title", { company })}</Text>
        <SkeletonList rows={6} />
      </Stack>
    </Tagged>
  );
}
