import { useTranslation } from "react-i18next";

import { Tagged, Text } from "~/lib/ui";

import { whyText, type Waiting } from "../../api/waiting";

/** Why a site still waits, in one plain sentence; nothing when the app has no reason to give. */
export function WhyLine({ waiting }: { waiting: Waiting | null | undefined }) {
  const { t, i18n } = useTranslation("sources");
  if (waiting === undefined || waiting === null) {
    return null;
  }
  return (
    <Tagged testId="boards-admin-why">
      <Text tone="secondary">{whyText(waiting, t, i18n.language)}</Text>
    </Tagged>
  );
}
