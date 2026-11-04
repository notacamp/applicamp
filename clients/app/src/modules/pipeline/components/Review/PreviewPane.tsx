import { useTranslation } from "react-i18next";

import { Banner, Button, ButtonLink, Cluster, PdfPages, Stack, Tagged, Text } from "~/lib/ui";

import { cvPreviewUrl, cvText, type CvDraft } from "../../api/cv";
import { languageName } from "../../api/labels";

/**
 * The draft as the PDF it will be: every page drawn on the page, made again
 * after every change, with its length and language in one line, Download
 * draft, and one plain sentence when a job site might not read it well.
 */
export function PreviewPane({
  opportunity,
  company,
  draft,
  onRetry,
  busy,
}: {
  opportunity: string;
  /** The company, as the job names it. */
  company: string;
  draft: CvDraft;
  /** Make the PDF again, in the same layout. */
  onRetry: () => void;
  /** Something is running. */
  busy: boolean;
}) {
  const n = draft.n;
  const language = draft.language;
  const preview = draft.preview;
  const { t, i18n } = useTranslation("pipeline");
  const ready = preview?.ready === true;
  const error = preview?.error ?? undefined;
  const pages = preview?.pages ?? preview?.length ?? 1;
  const checks = preview?.ready === true ? (preview.checks ?? undefined) : undefined;
  // What a job site reads out of the PDF: when it would miss something, say so.
  const lacking =
    checks !== undefined &&
    (checks.sections.missing.length > 0 ||
      checks.keywords.missing.length > 0 ||
      checks.lines.missing.length > 0 ||
      !checks.text_layer ||
      !checks.contact.name ||
      !checks.contact.email ||
      !checks.contact.links);
  return (
    <Tagged testId="cv-preview">
      <Stack>
        <Cluster>
          <Tagged testId="cv-preview-facts">
            <Text tone="secondary" size="sm">
              {t("cv.preview.facts", {
                pages: t("cv.layout.pages", { count: pages }),
                language: languageName(language, i18n.language),
              })}
            </Text>
          </Tagged>
          {ready ? (
            <ButtonLink
              variant="secondary"
              icon="download"
              href={cvPreviewUrl(opportunity, n, { download: true })}
              data-testid="cv-preview-download"
            >
              {t("cv.preview.download")}
            </ButtonLink>
          ) : null}
        </Cluster>
        {lacking ? (
          <Tagged testId="cv-preview-note">
            <Text tone="secondary" size="sm">
              {t("cv.preview.note")}
            </Text>
          </Tagged>
        ) : null}
        {error === undefined ? null : (
          <Tagged testId="cv-preview-error">
            <Banner
              tone="danger"
              icon="warning"
              title={
                error === "cv_doesnt_fit"
                  ? t("cv.preview.doesntFit")
                  : t("cv.preview.unavailable", { company })
              }
            >
              <Cluster>
                <Button
                  variant="secondary"
                  disabled={busy}
                  data-testid="cv-preview-retry"
                  onClick={onRetry}
                >
                  {t("cv.preview.retry")}
                </Button>
                <ButtonLink
                  variant="secondary"
                  icon="download"
                  href={`data:text/plain;charset=utf-8,${encodeURIComponent(cvText(draft))}`}
                  download={t("cv.preview.textFile", { company })}
                  data-testid="cv-preview-text"
                >
                  {t("cv.preview.downloadText")}
                </ButtonLink>
              </Cluster>
            </Banner>
          </Tagged>
        )}
        {ready ? (
          <PdfPages
            url={cvPreviewUrl(opportunity, n, { version: preview.at ?? null })}
            testId="cv-preview-pdf"
            labels={{
              loading: t("cv.preview.loading"),
              failed: t("cv.preview.failed"),
              page: (page, total) => t("cv.viewer.page", { page, pages: total }),
            }}
          />
        ) : error === undefined ? (
          <Tagged testId="cv-preview-making">
            <Text tone="secondary">{t("cv.preview.making")}</Text>
          </Tagged>
        ) : null}
      </Stack>
    </Tagged>
  );
}
