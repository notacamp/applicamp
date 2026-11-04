import { useTranslation } from "react-i18next";

import { Pill, Tag, Tagged } from "~/lib/ui";

import type { RowStatus } from "../../api/admin-rows";

/**
 * Where a board stands: reading and paused are a quiet tag, waiting for you asks you to decide,
 * being checked is a review, and not allowed is a tag in the danger tone.
 */
export function StageLabel({ status }: { status: RowStatus }) {
  const { t } = useTranslation("sources");
  const label = t(`admin.status.${status}`);
  switch (status) {
    case "waiting": {
      return (
        <Tagged testId="boards-admin-status-waiting">
          <Pill signal="decide">{label}</Pill>
        </Tagged>
      );
    }
    case "checking": {
      return (
        <Tagged testId="boards-admin-status-checking">
          <Pill signal="review">{label}</Pill>
        </Tagged>
      );
    }
    case "notAllowed": {
      return (
        <Tagged testId="boards-admin-status-notAllowed">
          <Tag tone="danger">{label}</Tag>
        </Tagged>
      );
    }
    case "reading":
    case "paused": {
      return (
        <Tagged testId={`boards-admin-status-${status}`}>
          <Tag>{label}</Tag>
        </Tagged>
      );
    }
  }
}
