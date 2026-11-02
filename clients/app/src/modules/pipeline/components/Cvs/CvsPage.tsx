import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  ButtonLink,
  Cluster,
  formatSize,
  Label,
  PdfViewer,
  Placeholder,
  RowItem,
  RowList,
  Stack,
  Tagged,
  Text,
  ViewSwitch,
} from "~/lib/ui";

import { cvPdfUrl, useCvs, type CvListItem } from "../../api/cv";
import { letterPdfUrl } from "../../api/letter";
import { formatShortDate } from "../../api/labels";

type Filter = "all" | "cvs" | "letters" | "drafts";

const FILTERS = ["all", "cvs", "letters", "drafts"] as const satisfies readonly Filter[];

/** What a row opens in the viewer. */
interface Viewing {
  document: CvListItem["document"];
  opportunity: string;
  version: number;
  company: string;
  item: CvListItem;
}

export interface CvsPageProps {
  /** Review a draft CV (`/pipeline/$id/cv`). */
  onReview: (opportunity: string) => void;
  /** Review a draft cover letter (`/pipeline/$id/letter`). */
  onReviewLetter: (opportunity: string) => void;
  /** Open the job in the pipeline. */
  onOpportunity: (opportunity: string) => void;
  /** Go to the pipeline, to make a first CV. */
  onPipeline: () => void;
}

const keyOf = (item: CvListItem) =>
  `${item.document}:${item.opportunity_id}:${item.kind}:${String(item.version ?? item.draft)}`;

/** Whether `item` shows under `filter`: CVs or letters, or the drafts in review. */
function shown(item: CvListItem, filter: Filter): boolean {
  switch (filter) {
    case "all": {
      return true;
    }
    case "cvs": {
      return item.document === "cv";
    }
    case "letters": {
      return item.document === "letter";
    }
    case "drafts": {
      return item.kind === "draft";
    }
  }
}

/** Where a row's PDF is. */
function pdfUrl(
  document: CvListItem["document"],
  opportunity: string,
  version: number,
  download = false,
) {
  return document === "letter"
    ? letterPdfUrl(opportunity, version, download)
    : cvPdfUrl(opportunity, version, download);
}

/**
 * `/cvs`: every CV across the opportunities, newest first: the PDFs that were
 * made (open one in the viewer, download it) and the drafts being reviewed
 * (go on reviewing), cover letters next to the CVs. Filtered to all, CVs,
 * letters or drafts.
 */
export function CvsPage({ onReview, onReviewLetter, onOpportunity, onPipeline }: CvsPageProps) {
  const { t, i18n } = useTranslation("pipeline");
  const list = useCvs();
  const [filter, setFilter] = useState<Filter>("all");
  const [viewing, setViewing] = useState<Viewing | null>(null);

  const items = list.data?.items ?? [];
  const rows = items.filter((item) => shown(item, filter));

  let body: ReactNode;
  if (list.isPending) {
    body = (
      <Tagged testId="cvs-loading">
        <Text tone="secondary">{t("cvs.loading")}</Text>
      </Tagged>
    );
  } else if (list.isError) {
    body = (
      <Tagged testId="cvs-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("cvs.loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void list.refetch();
              }}
            >
              {t("retry")}
            </Button>
          }
        />
      </Tagged>
    );
  } else if (items.length === 0) {
    body = (
      <Placeholder
        testId="cvs-empty"
        icon="document"
        title={t("cvs.empty.title")}
        action={
          <Button
            variant="primary"
            icon="arrow-right"
            data-testid="cvs-empty-pipeline"
            onClick={onPipeline}
          >
            {t("cvs.empty.action")}
          </Button>
        }
      >
        {t("cvs.empty.body")}
      </Placeholder>
    );
  } else if (rows.length === 0) {
    body = (
      <Placeholder testId="cvs-no-results" icon="search" title={t("cvs.emptyFiltered.title")}>
        {t("cvs.emptyFiltered.body")}
      </Placeholder>
    );
  } else {
    body = (
      <RowList label={t("cvs.label")} testId="cvs-list">
        {rows.map((item) => {
          const version = item.version ?? undefined;
          const pdf = item.kind === "version" && version !== undefined;
          const template = item.template ?? undefined;
          const letter = item.document === "letter";
          return (
            <RowItem
              key={keyOf(item)}
              current={item.current}
              testId="cvs-row"
              title={
                <Cluster>
                  <span data-testid="cvs-row-title">
                    {t(letter ? "cvs.letterTitle" : "cvs.title", {
                      role: item.title,
                      company: item.company,
                    })}
                  </span>
                  <Tagged testId="cvs-row-document">
                    <Label hue={letter ? "purple" : "gray"}>
                      {t(letter ? "cvs.letterLabel" : "cvs.cvLabel")}
                    </Label>
                  </Tagged>
                  <Tagged testId="cvs-row-status">
                    <Label hue={item.status === "approved" ? "green" : "blue"}>
                      {t(item.status === "approved" ? "cvs.approved" : "cvs.draft")}
                    </Label>
                  </Tagged>
                </Cluster>
              }
              subtitle={
                <Tagged testId="cvs-row-version">
                  <span>
                    {version === undefined
                      ? `${t("cvs.draftN", { n: item.draft })}${
                          item.status === "approved" ? ` · ${t("cvs.noPdf")}` : ""
                        }`
                      : `${t("cvs.version", { version })}${item.current ? ` · ${t("cvs.current")}` : ""}`}
                    {" · "}
                    {t(pdf ? "cvs.made" : "cvs.started", {
                      date: formatShortDate(item.created_at, i18n.language),
                    })}
                  </span>
                </Tagged>
              }
              meta={
                pdf && (letter || template !== undefined) ? (
                  <>
                    <Tagged testId="cvs-row-facts">
                      <span>
                        {template === undefined
                          ? t("cvs.letterFacts", {
                              count: item.pages ?? 1,
                              size: formatSize(item.size_bytes ?? 0, i18n.language),
                            })
                          : t("cvs.facts", {
                              template: t(`cv.layout.templates.${template}`),
                              count: item.pages ?? 1,
                              size: formatSize(item.size_bytes ?? 0, i18n.language),
                            })}
                      </span>
                    </Tagged>
                    <span>
                      {item.downloaded_at === undefined || item.downloaded_at === null
                        ? t("cvs.neverDownloaded")
                        : t("cvs.downloaded", {
                            date: formatShortDate(item.downloaded_at, i18n.language),
                          })}
                    </span>
                  </>
                ) : undefined
              }
              actions={
                <>
                  {pdf ? (
                    <>
                      <Button
                        variant="secondary"
                        data-testid="cvs-view"
                        onClick={() => {
                          setViewing({
                            document: item.document,
                            opportunity: item.opportunity_id,
                            version,
                            company: item.company,
                            item,
                          });
                        }}
                      >
                        {t("cvs.viewPdf")}
                      </Button>
                      <ButtonLink
                        variant="secondary"
                        icon="download"
                        href={pdfUrl(item.document, item.opportunity_id, version, true)}
                        data-testid="cvs-download"
                      >
                        {t("cvs.download")}
                      </ButtonLink>
                    </>
                  ) : (
                    <Button
                      variant="secondary"
                      data-testid="cvs-review"
                      onClick={() => {
                        (letter ? onReviewLetter : onReview)(item.opportunity_id);
                      }}
                    >
                      {t("cvs.review")}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    data-testid="cvs-opportunity"
                    onClick={() => {
                      onOpportunity(item.opportunity_id);
                    }}
                  >
                    {t("cvs.opportunity")}
                  </Button>
                </>
              }
            />
          );
        })}
      </RowList>
    );
  }

  const open = viewing !== null;
  const detail = viewing?.item;
  return (
    <Stack>
      {items.length === 0 ? null : (
        <Tagged testId="cvs-filter">
          <ViewSwitch
            label={t("cvs.filter.label")}
            options={FILTERS.map((value) => ({
              value,
              label: t(`cvs.filter.${value}`),
              testId: `cvs-filter-${value}`,
            }))}
            value={filter}
            onChange={setFilter}
          />
        </Tagged>
      )}
      {body}
      <PdfViewer
        open={open}
        onClose={() => {
          setViewing(null);
        }}
        url={pdfUrl(viewing?.document ?? "cv", viewing?.opportunity ?? "", viewing?.version ?? 0)}
        downloadUrl={pdfUrl(
          viewing?.document ?? "cv",
          viewing?.opportunity ?? "",
          viewing?.version ?? 0,
          true,
        )}
        title={t(viewing?.document === "letter" ? "cvs.letterViewer.title" : "cvs.viewer.title", {
          company: viewing?.company ?? "",
          version: viewing?.version ?? 0,
        })}
        subtitle={
          detail === undefined
            ? undefined
            : detail.template === undefined || detail.template === null
              ? t("letter.viewer.subtitle", {
                  date: formatShortDate(detail.created_at, i18n.language),
                  pages: t("cv.layout.pages", { count: detail.pages ?? 1 }),
                  size: formatSize(detail.size_bytes ?? 0, i18n.language),
                })
              : t("cv.viewer.subtitle", {
                  date: formatShortDate(detail.created_at, i18n.language),
                  template: t(`cv.layout.templates.${detail.template}`),
                  pages: t("cv.layout.pages", { count: detail.pages ?? 1 }),
                  size: formatSize(detail.size_bytes ?? 0, i18n.language),
                })
        }
        labels={{
          close: t("cv.viewer.close"),
          download: t("cv.pdf.download"),
          loading: t("cv.viewer.loading"),
          failed: t("cv.viewer.failed"),
          page: (page, pages) => t("cv.viewer.page", { page, pages }),
        }}
      />
    </Stack>
  );
}
