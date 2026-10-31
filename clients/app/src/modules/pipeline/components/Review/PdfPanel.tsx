import { useTranslation } from "react-i18next";

import {
  Button,
  ButtonLink,
  Cluster,
  formatSize,
  Icon,
  Label,
  RowItem,
  RowList,
  Section,
  Stack,
  Tagged,
  Text,
} from "~/lib/ui";

import { cvPdfUrl, type CvPdfChecks, type CvVersion } from "../../api/cv";
import { formatShortDate } from "../../api/labels";

/** One line of the read-back: a tick and what was found. */
function Check({ id, children, detail }: { id: string; children: string; detail?: string }) {
  return (
    <Tagged testId={`cv-pdf-check-${id}`}>
      <Stack gap="sm">
        <Cluster>
          <Icon name="check" />
          <Text>{children}</Text>
        </Cluster>
        {detail === undefined ? null : (
          <Text tone="secondary" size="sm">
            {detail}
          </Text>
        )}
      </Stack>
    </Tagged>
  );
}

/** What the read-back found in a PDF: the five lines of the prototype's PDF tab. */
export function PdfChecks({ checks }: { checks: CvPdfChecks }) {
  const { t, i18n } = useTranslation("pipeline");
  return (
    <Tagged testId="cv-pdf-checks">
      <Stack gap="sm">
        <Text weight="semibold">{t("cv.pdf.checksTitle")}</Text>
        <Check id="sections" detail={checks.sections.expected.join(", ")}>
          {t("cv.pdf.sections", { count: checks.sections.expected.length })}
        </Check>
        <Check id="keywords">
          {checks.keywords.total === 0
            ? t("cv.pdf.noKeywords")
            : t("cv.pdf.keywords", { count: checks.keywords.found })}
        </Check>
        <Check id="text">{t("cv.pdf.textLayer")}</Check>
        <Check id="contact">{t("cv.pdf.contact")}</Check>
        <Check id="size">
          {t("cv.pdf.size", {
            count: checks.size.pages,
            size: formatSize(checks.size.bytes, i18n.language),
          })}
        </Check>
      </Stack>
    </Tagged>
  );
}

/** "Open the PDF", Download and the read-back: the aside of an approved CV. */
export function PdfPanel({
  opportunity,
  version,
  onOpen,
}: {
  opportunity: string;
  version: CvVersion;
  onOpen: (version: number) => void;
}) {
  const { t } = useTranslation("pipeline");
  return (
    <Tagged testId="cv-pdf">
      <Stack>
        <Cluster>
          <Button
            variant="primary"
            icon="arrow-right"
            data-testid="cv-open-pdf"
            onClick={() => {
              onOpen(version.version);
            }}
          >
            {t("cv.pdf.open")}
          </Button>
          <ButtonLink
            variant="secondary"
            icon="download"
            href={cvPdfUrl(opportunity, version.version, true)}
            data-testid="cv-download-pdf"
          >
            {t("cv.pdf.download")}
          </ButtonLink>
        </Cluster>
        <PdfChecks checks={version.checks} />
      </Stack>
    </Tagged>
  );
}

/** Every PDF made from this CV, newest first: view one, download one. */
export function VersionsSection({
  opportunity,
  versions,
  onOpen,
}: {
  opportunity: string;
  versions: readonly CvVersion[];
  onOpen: (version: number) => void;
}) {
  const { t, i18n } = useTranslation("pipeline");
  if (versions.length === 0) {
    return null;
  }
  return (
    <Section
      title={t("cv.versions.title")}
      description={t("cv.versions.lead")}
      testId="cv-versions"
    >
      <RowList label={t("cv.versions.title")}>
        {versions.map((version) => (
          <RowItem
            key={version.version}
            current={version.current}
            testId={`cv-version-${String(version.version)}`}
            title={
              <Cluster>
                <span>
                  {t("cv.versions.name", {
                    version: version.version,
                    template: t(`cv.layout.templates.${version.template}`),
                  })}
                </span>
                {version.current ? <Label hue="green">{t("cv.versions.current")}</Label> : null}
              </Cluster>
            }
            subtitle={t("cv.versions.facts", {
              count: version.pages,
              size: formatSize(version.size_bytes, i18n.language),
              made: formatShortDate(version.created_at, i18n.language),
            })}
            meta={
              version.downloaded_at === undefined || version.downloaded_at === null
                ? t("cv.versions.neverDownloaded")
                : t("cv.versions.downloaded", {
                    date: formatShortDate(version.downloaded_at, i18n.language),
                  })
            }
            actions={
              <>
                <Button
                  variant="secondary"
                  data-testid={`cv-version-view-${String(version.version)}`}
                  onClick={() => {
                    onOpen(version.version);
                  }}
                >
                  {t("cv.versions.view")}
                </Button>
                <ButtonLink
                  variant="secondary"
                  icon="download"
                  href={cvPdfUrl(opportunity, version.version, true)}
                  data-testid={`cv-version-download-${String(version.version)}`}
                >
                  {t("cv.pdf.download")}
                </ButtonLink>
              </>
            }
          />
        ))}
      </RowList>
    </Section>
  );
}
