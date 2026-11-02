import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Bullets,
  Button,
  ButtonLink,
  Cluster,
  formatSize,
  Stack,
  Tagged,
  Text,
} from "~/lib/ui";

import { formatCost, formatLimit } from "../../api/labels";
import { letterPdfUrl, type Letter, type LetterDraft } from "../../api/letter";

/** The state of the check, "At a glance" and what it cost. */
export function LetterAside({
  letter,
  draft,
  spend,
  onFirst,
  onBack,
  onOpenPdf,
}: {
  letter: Letter;
  draft: LetterDraft;
  /** The month's AI spend line (the sources module's). */
  spend: ReactNode;
  /** Go to the first thing that needs the person. */
  onFirst: () => void;
  /** Back to the pipeline, once approved. */
  onBack: () => void;
  /** Open a version's PDF. */
  onOpenPdf: (version: number) => void;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const newest = letter.versions[0];
  const caps = letter.caps;
  const money = (euros: number) => formatCost(euros, i18n.language);
  const glance: string[] = [
    draft.cv_version === undefined || draft.cv_version === null
      ? t("letter.aside.words", { count: draft.words })
      : t("letter.aside.wordsCv", { count: draft.words, cv: draft.cv_version }),
    draft.company_facts.length === 0
      ? t("letter.aside.noFacts")
      : t("letter.aside.facts", {
          count: draft.company_facts.length,
          list: draft.company_facts.map((fact) => fact.text).join("; "),
        }),
  ];
  return (
    <>
      {draft.status === "approved" ? (
        <Tagged testId="letter-banner-approved">
          <Banner
            icon="check"
            title={t("letter.aside.approvedTitle", { version: draft.version ?? 1 })}
          >
            <Stack gap="sm">
              <Text>{t("letter.aside.approvedBody")}</Text>
              <Cluster>
                <Button variant="secondary" data-testid="letter-back-to-pipeline" onClick={onBack}>
                  {t("letter.aside.toPipeline")}
                </Button>
              </Cluster>
            </Stack>
          </Banner>
        </Tagged>
      ) : null}
      {draft.status === "approved" && newest !== undefined ? (
        <Tagged testId="letter-pdf">
          <Stack>
            <Cluster>
              <Button
                variant="primary"
                icon="arrow-right"
                data-testid="letter-open-pdf"
                onClick={() => {
                  onOpenPdf(newest.version);
                }}
              >
                {t("letter.pdf.open")}
              </Button>
              <ButtonLink
                variant="secondary"
                icon="download"
                href={letterPdfUrl(letter.opportunity_id, newest.version, true)}
                data-testid="letter-download-pdf"
              >
                {t("letter.pdf.download")}
              </ButtonLink>
            </Cluster>
            <Text tone="secondary" size="sm">
              {t("letter.pdf.size", {
                count: newest.pages,
                size: formatSize(newest.size_bytes, i18n.language),
              })}
            </Text>
          </Stack>
        </Tagged>
      ) : null}
      {draft.status === "approved" ? null : draft.flagged > 0 ? (
        <Tagged testId="letter-banner-flagged">
          <Banner
            tone="danger"
            icon="warning"
            title={t("letter.aside.flagged", { count: draft.flagged })}
          >
            <Stack gap="sm">
              <Text>{t("letter.aside.flaggedBody")}</Text>
              <Cluster>
                <Button variant="secondary" data-testid="letter-first-flagged" onClick={onFirst}>
                  {t("letter.aside.goFirst")}
                </Button>
              </Cluster>
            </Stack>
          </Banner>
        </Tagged>
      ) : (
        <Tagged testId="letter-banner-clean">
          <Banner icon="check" title={t("letter.aside.clean")}>
            {t("letter.aside.cleanBody")}
          </Banner>
        </Tagged>
      )}
      <Tagged testId="letter-glance">
        <Stack gap="sm">
          <Text weight="semibold">{t("letter.aside.glance")}</Text>
          <Bullets items={glance} />
        </Stack>
      </Tagged>
      <Stack gap="sm">
        <Tagged testId="letter-drafts-today">
          <Text tone="secondary" size="sm">
            {t("letter.spend.drafts", { n: caps.drafts_today, cap: caps.drafts_today_cap })}
          </Text>
        </Tagged>
        <Tagged testId="letter-spend">
          <Text tone="secondary" size="sm">
            {caps.day_limit_eur === undefined || caps.day_limit_eur === null
              ? t("letter.spend.dayNoLimit", {
                  cost: money(letter.draft_eur),
                  spent: money(caps.today_eur),
                })
              : t("letter.spend.day", {
                  cost: money(letter.draft_eur),
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
