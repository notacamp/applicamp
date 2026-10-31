import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Bullets,
  Button,
  ChipList,
  Cluster,
  Label,
  Stack,
  Tagged,
  Text,
  type LabelHue,
} from "~/lib/ui";

import type { Cv, CvDraft, CvKeyword } from "../../api/cv";
import { PdfPanel } from "./PdfPanel";
import { formatCost, formatLimit } from "../../api/labels";

const STATE_HUES: Record<CvKeyword["state"], LabelHue> = {
  on_cv: "green",
  in_profile: "yellow",
  not_in_profile: "gray",
};

function KeywordGroup({
  state,
  keywords,
}: {
  state: CvKeyword["state"];
  keywords: readonly CvKeyword[];
}) {
  const { t } = useTranslation("pipeline");
  const shown = keywords.filter((keyword) => keyword.state === state);
  if (shown.length === 0) {
    return null;
  }
  return (
    <Tagged testId={`cv-keywords-${state}`}>
      <Stack gap="sm">
        <Text tone="secondary" size="sm">
          {t(`cv.keywords.${state}`)}
        </Text>
        <ChipList>
          {shown.map((keyword) => (
            <Label key={`${keyword.kind}-${keyword.label}`} hue={STATE_HUES[state]}>
              {keyword.level === "nice"
                ? t("cv.keywords.nice", { label: keyword.label })
                : keyword.label}
            </Label>
          ))}
        </ChipList>
      </Stack>
    </Tagged>
  );
}

/** The state of the check, the job's keywords, what changed and what it cost. */
export function ReviewAside({
  cv,
  draft,
  spend,
  onFirst,
  onBack,
  onOpenPdf,
}: {
  cv: Cv;
  draft: CvDraft;
  /** The month's AI spend line (the sources module's). */
  spend: ReactNode;
  /** Go to the first flagged line. */
  onFirst: () => void;
  /** Back to the pipeline, once approved. */
  onBack: () => void;
  /** Open a version's PDF. */
  onOpenPdf: (version: number) => void;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const { changes, keywords } = draft;
  const nextVersion = (cv.versions[0]?.version ?? 0) + 1;
  const newest = cv.versions[0];
  const lines: string[] = [
    t("cv.changes.reworded", { count: changes.reworded }),
    t("cv.changes.movedUp", { count: changes.moved_up }),
    ...(changes.edited > 0 ? [t("cv.changes.edited", { count: changes.edited })] : []),
    t("cv.changes.asIs", { count: changes.as_in_profile }),
    ...(changes.left_out_roles > 0
      ? [t("cv.changes.leftOutRoles", { count: changes.left_out_roles })]
      : []),
    ...(changes.left_out_lines > 0
      ? [t("cv.changes.leftOutLines", { count: changes.left_out_lines })]
      : []),
  ];
  const caps = cv.caps;
  const money = (euros: number) => formatCost(euros, i18n.language);
  return (
    <>
      {draft.status === "approved" ? (
        <Tagged testId="cv-banner-approved">
          <Banner icon="check" title={t("cv.aside.approvedTitle", { version: draft.version ?? 1 })}>
            <Stack gap="sm">
              <Text>{t("cv.aside.approvedBody")}</Text>
              <Cluster>
                <Button variant="secondary" data-testid="cv-back-to-pipeline" onClick={onBack}>
                  {t("cv.aside.toPipeline")}
                </Button>
              </Cluster>
            </Stack>
          </Banner>
        </Tagged>
      ) : null}
      {draft.status === "approved" && newest !== undefined ? (
        <PdfPanel opportunity={cv.opportunity_id} version={newest} onOpen={onOpenPdf} />
      ) : null}
      {draft.status === "approved" ? null : draft.flagged > 0 ? (
        <Tagged testId="cv-banner-flagged">
          <Banner
            tone="danger"
            icon="warning"
            title={t("cv.aside.flagged", { count: draft.flagged })}
          >
            <Stack gap="sm">
              <Text>{t("cv.aside.flaggedBody")}</Text>
              <Cluster>
                <Button variant="secondary" data-testid="cv-first-flagged" onClick={onFirst}>
                  {t("cv.aside.goFirst")}
                </Button>
              </Cluster>
            </Stack>
          </Banner>
        </Tagged>
      ) : (
        <Tagged testId="cv-banner-clean">
          <Banner icon="check" title={t("cv.aside.clean")}>
            {t("cv.aside.cleanBody", { version: nextVersion })}
          </Banner>
        </Tagged>
      )}
      <Tagged testId="cv-keywords">
        <Stack gap="sm">
          <Text weight="semibold">
            {t("cv.keywords.title")}{" "}
            <Tagged testId="cv-keywords-count">
              <span>{t("cv.keywords.count", { on: keywords.on_cv, total: keywords.total })}</span>
            </Tagged>
          </Text>
          {keywords.items.length === 0 ? (
            <Text tone="secondary" size="sm">
              {t("cv.keywords.none")}
            </Text>
          ) : (
            <>
              <KeywordGroup state="on_cv" keywords={keywords.items} />
              <KeywordGroup state="in_profile" keywords={keywords.items} />
              <KeywordGroup state="not_in_profile" keywords={keywords.items} />
              <Text tone="secondary" size="sm">
                {t("cv.keywords.note")}
              </Text>
            </>
          )}
        </Stack>
      </Tagged>
      <Tagged testId="cv-changes">
        <Stack gap="sm">
          <Text weight="semibold">{t("cv.changes.title")}</Text>
          <Bullets items={lines} />
        </Stack>
      </Tagged>
      <Stack gap="sm">
        <Tagged testId="cv-drafts-today">
          <Text tone="secondary" size="sm">
            {t("cv.spend.drafts", { n: caps.drafts_today, cap: caps.drafts_today_cap })}
          </Text>
        </Tagged>
        <Tagged testId="cv-spend">
          <Text tone="secondary" size="sm">
            {caps.day_limit_eur === undefined || caps.day_limit_eur === null
              ? t("cv.spend.dayNoLimit", { spent: money(caps.today_eur) })
              : t("cv.spend.day", {
                  spent: money(caps.today_eur),
                  limit: formatLimit(caps.day_limit_eur, i18n.language),
                })}
          </Text>
        </Tagged>
        {spend}
      </Stack>
    </>
  );
}
