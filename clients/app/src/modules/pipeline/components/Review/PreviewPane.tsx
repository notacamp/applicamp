import { useTranslation } from "react-i18next";

import { Banner, Button, ButtonLink, Cluster, PdfPages, Stack, Tagged, Text } from "~/lib/ui";

import { cvPreviewUrl, type CvPreview } from "../../api/cv";
import { languageName } from "../../api/labels";

/**
 * The draft as the PDF it will be: every page drawn on the page, made again
 * after every change, with its length and language in one line, Download
 * draft, and one plain sentence when a job site might not read it well. When
 * there is no PDF yet, or it is older than the last change, "Make the PDF".
 */
export function PreviewPane({
  opportunity,
  n,
  language,
  preview,
  onMake,
  making,
  disabled,
}: {
  opportunity: string;
  /** The draft. */
  n: number;
  /** The CV's language tag. */
  language: string;
  preview: CvPreview | null | undefined;
  /** Make the PDF of the draft as it is now, in the layout chosen. */
  onMake: () => void;
  /** The PDF is being made. */
  making: boolean;
  disabled: boolean;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const ready = preview?.ready === true;
  const error = preview?.error ?? undefined;
  const pages = preview?.pages ?? preview?.length ?? 1;
  const checks = preview?.ready === true ? (preview.checks ?? undefined) : undefined;
  const make = (
    <Button
      variant="secondary"
      disabled={disabled || making}
      data-testid="cv-preview-make"
      onClick={onMake}
    >
      {making ? t("cv.preview.making") : t("cv.preview.make")}
    </Button>
  );
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
        {preview?.stale === true ? (
          <Tagged testId="cv-preview-stale">
            <Banner icon="warning" title={t("cv.preview.stale")} action={make} />
          </Tagged>
        ) : null}
        {error === undefined ? null : (
          <Tagged testId="cv-preview-error">
            <Banner
              tone="danger"
              icon="warning"
              title={t(
                error === "cv_doesnt_fit" ? "cv.preview.doesntFit" : "cv.preview.unavailable",
              )}
            />
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
          <Tagged testId="cv-preview-none">
            <Stack gap="sm">
              <Text tone="secondary">{t("cv.preview.none")}</Text>
              <Cluster>{make}</Cluster>
            </Stack>
          </Tagged>
        ) : null}
      </Stack>
    </Tagged>
  );
}
