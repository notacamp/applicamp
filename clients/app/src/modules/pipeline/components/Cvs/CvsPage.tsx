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
import { formatShortDate } from "../../api/labels";

type Filter = "all" | "approved" | "drafts";

/** What a row opens in the viewer. */
interface Viewing {
  opportunity: string;
  version: number;
  company: string;
  item: CvListItem;
}

export interface CvsPageProps {
  /** Review a draft (`/pipeline/$id/cv`). */
  onReview: (opportunity: string) => void;
  /** Open the job in the pipeline. */
  onOpportunity: (opportunity: string) => void;
  /** Go to the pipeline, to make a first CV. */
  onPipeline: () => void;
}

const keyOf = (item: CvListItem) =>
  `${item.opportunity_id}:${item.kind}:${String(item.version ?? item.draft)}`;

/** Whether `item` shows under `filter`: approved ones are the PDFs, drafts the ones in review. */
function shown(item: CvListItem, filter: Filter): boolean {
  if (filter === "all") {
    return true;
  }
  return filter === "approved" ? item.kind === "version" : item.kind === "draft";
}

/**
 * `/cvs`: every CV across the opportunities, newest first: the PDFs that were
 * made (open one in the viewer, download it) and the drafts being reviewed
 * (go on reviewing). Filtered to all, approved or drafts.
 */
export function CvsPage({ onReview, onOpportunity, onPipeline }: CvsPageProps) {
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
          return (
            <RowItem
              key={keyOf(item)}
              current={item.current}
              testId="cvs-row"
              title={
                <Cluster>
                  <span data-testid="cvs-row-title">
                    {t("cvs.title", { role: item.title, company: item.company })}
                  </span>
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
                pdf && template !== undefined ? (
                  <>
                    <Tagged testId="cvs-row-facts">
                      <span>
                        {t("cvs.facts", {
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
                        href={cvPdfUrl(item.opportunity_id, version, true)}
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
                        onReview(item.opportunity_id);
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
            options={(["all", "approved", "drafts"] as const).map((value) => ({
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
        url={cvPdfUrl(viewing?.opportunity ?? "", viewing?.version ?? 0)}
        downloadUrl={cvPdfUrl(viewing?.opportunity ?? "", viewing?.version ?? 0, true)}
        title={t("cvs.viewer.title", {
          company: viewing?.company ?? "",
          version: viewing?.version ?? 0,
        })}
        subtitle={
          detail?.template === undefined || detail.template === null
            ? undefined
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
