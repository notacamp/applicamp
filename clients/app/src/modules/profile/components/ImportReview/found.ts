import { useTranslation } from "react-i18next";

import type { FoundKey } from "../../api/sections";

/**
 * "13 jobs, 31 achievements, 14 skills, 1 degree and 3 languages", in the
 * page's language: what an import found, for the review and for Complete
 * your profile. Empty when it found nothing.
 */
export function useFoundList() {
  const { t, i18n } = useTranslation("profile");
  return (items: readonly { key: FoundKey; count: number }[]): string =>
    new Intl.ListFormat(i18n.language, { style: "long", type: "conjunction" }).format(
      items.map(({ key, count }) => t(`review.counts.${key}`, { count })),
    );
}
