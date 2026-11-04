import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import {
  ActionDock,
  Banner,
  Bullets,
  Button,
  Cluster,
  Dialog,
  Disclosure,
  formatSize,
  Label,
  Page,
  PdfViewer,
  ReviewLayout,
  Section,
  Stack,
  Tagged,
  Text,
  TouchTargets,
} from "~/lib/ui";

import {
  cvErrorCode,
  cvPdfUrl,
  DEFAULT_LAYOUT,
  layoutOf,
  pdfFailureReasons,
  previewLayout,
  useApprove,
  useChangeLine,
  useConfirmLine,
  useCv,
  useMakeDraft,
  useMakeVersion,
  usePreview,
  usePutBack,
  useUnapprove,
  type Cv,
  type CvDraft,
  type CvLayout,
} from "../../api/cv";
import { formatCost, formatShortDate } from "../../api/labels";
import { useOpportunity } from "../../api/opportunities";
import { titleOf } from "../PipelinePage/OpportunityViews";
import { LetterSlot } from "../Letter/LetterSlot";
import { CvSheet } from "./CvSheet";
import { DraftSlot } from "./DraftSlot";
import { cvLanguage, LayoutControls } from "./LayoutControls";
import type { LineActions } from "./LineRow";
import { VersionsSection } from "./PdfPanel";
import { PreviewPane } from "./PreviewPane";
import { ReviewAside } from "./ReviewAside";

export interface ReviewPageProps {
  /** The opportunity. */
  id: string;
  /** Back to Prepare. */
  onBack: () => void;
  /** Back to the pipeline (once approved). */
  onPipeline: () => void;
  /** The month's AI spend line. */
  spend?: ReactNode;
  /** Open the cover letter's review; none hides "Write the cover letter". */
  onLetter?: ((id: string) => void) | undefined;
}

/** A language tag as a name in the person's language ("English"). */
export function languageName(tag: string, display: string): string {
  try {
    return new Intl.DisplayNames([display], { type: "language" }).of(tag) ?? tag;
  } catch {
    return tag;
  }
}

/**
 * `/pipeline/$id/cv`: the review of a tailored CV. The draft as the PDF it
 * will be, made again after every change; beside it the questions the app
 * could not answer by itself (Yes, keep it / Change it / Remove it), the
 * job's keywords and what changed. Every line and where it comes from opens
 * below the PDF. Approve is off while a question waits.
 */
export function ReviewPage({ id, onBack, onPipeline, spend, onLetter }: ReviewPageProps) {
  const { t } = useTranslation("pipeline");
  const opportunity = useOpportunity(id);
  const cv = useCv(id);
  const title =
    opportunity.data === undefined ? t("cv.title") : titleOf(opportunity.data, t("untitled"));
  const company = opportunity.data?.company ?? "";

  let body: ReactNode;
  if (cv.isPending || opportunity.isPending) {
    body = (
      <Tagged testId="cv-loading">
        <Text tone="secondary">{t("cv.loading")}</Text>
      </Tagged>
    );
  } else if (cv.isError || opportunity.isError) {
    const missing =
      problemCode(cv.error) === "not_found" || problemCode(opportunity.error) === "not_found";
    body = (
      <Tagged testId="cv-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t(missing ? "cv.errors.not_found" : "cv.loadFailed")}
          action={
            missing ? undefined : (
              <Button
                variant="secondary"
                onClick={() => {
                  void cv.refetch();
                  void opportunity.refetch();
                }}
              >
                {t("retry")}
              </Button>
            )
          }
        />
      </Tagged>
    );
  } else if (cv.data.draft === undefined || cv.data.draft === null) {
    body = (
      <Tagged testId="cv-empty">
        <Stack>
          <Text tone="secondary">{t("cv.empty.body")}</Text>
          <DraftSlot id={id} testIdPrefix="cv" />
        </Stack>
      </Tagged>
    );
  } else {
    body = (
      <Review
        key={cv.data.draft.n}
        id={id}
        cv={cv.data}
        draft={cv.data.draft}
        spend={spend}
        onPipeline={onPipeline}
        onLetter={onLetter}
      />
    );
  }

  return (
    <Page
      testId="cv-review"
      wide
      eyebrow={t("cv.eyebrow")}
      title={company === "" ? title : t("cv.heading", { company })}
      lead={t("cv.lead")}
    >
      <Cluster>
        <Button variant="ghost" data-testid="cv-back" onClick={onBack}>
          {t("cv.back")}
        </Button>
      </Cluster>
      {body}
    </Page>
  );
}

function Review({
  id,
  cv,
  draft,
  spend,
  onPipeline,
  onLetter,
}: {
  id: string;
  cv: Cv;
  draft: CvDraft;
  spend: ReactNode;
  onPipeline: () => void;
  onLetter: ((id: string) => void) | undefined;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const change = useChangeLine(id, draft.n);
  const confirm = useConfirmLine(id, draft.n);
  const putBack = usePutBack(id, draft.n);
  const approve = useApprove(id, draft.n);
  const makeVersion = useMakeVersion(id);
  const preview = usePreview(id, draft.n);
  const unapprove = useUnapprove(id, draft.n);
  const undoing = useUndoWindow(draft.undo_until ?? undefined);
  const [language, setLanguage] = useState(() => cvLanguage(draft.language));
  const again = useMakeDraft(id, language);
  const editable = draft.status === "draft";
  const newest = cv.versions[0];
  // What the PDF is made with: the preview's before Approve, the newest version's after.
  const layout = editable
    ? (previewLayout(draft.preview) ?? DEFAULT_LAYOUT)
    : newest === undefined
      ? DEFAULT_LAYOUT
      : layoutOf(newest);
  // A layout asked for on an approved CV, waiting for the person to confirm.
  const [asked, setAsked] = useState<CvLayout | null>(null);
  const [viewing, setViewing] = useState<number | null>(null);
  const busy =
    change.isPending ||
    confirm.isPending ||
    putBack.isPending ||
    approve.isPending ||
    makeVersion.isPending ||
    preview.isPending ||
    unapprove.isPending ||
    again.isPending;
  const failure = [change, confirm, putBack, approve, makeVersion, preview, unapprove, again].find(
    (action) => action.isError,
  )?.error;
  const failedLength = preview.isError
    ? preview.variables.length
    : approve.isError
      ? layout.length
      : (asked ?? layout).length;

  const resetErrors = () => {
    for (const action of [
      change,
      confirm,
      putBack,
      approve,
      makeVersion,
      preview,
      unapprove,
      again,
    ]) {
      action.reset();
    }
  };
  const viewed = cv.versions.find((version) => version.version === viewing);
  const actions: LineActions = {
    editable,
    busy,
    edit: (line, text) => {
      resetErrors();
      change.mutate({ line, change: { text } });
    },
    remove: (line) => {
      resetErrors();
      change.mutate({ line, change: { remove: true } });
    },
    confirm: (line, text) => {
      resetErrors();
      confirm.mutate(text === undefined ? { line } : { line, text });
    },
    keep: (line) => {
      resetErrors();
      confirm.mutate({ line, proposal: true });
    },
  };
  const version = draft.version ?? (newest?.version ?? 0) + 1;
  const waiting = draft.waiting.length;
  const sheet = (
    <CvSheet
      draft={draft}
      actions={actions}
      onPutBack={(what) => {
        resetErrors();
        putBack.mutate(what);
      }}
    />
  );

  return (
    <TouchTargets>
      <Stack>
        <Tagged testId="cv-toolbar">
          <Cluster>
            <Tagged testId="cv-draft-number">
              <Text weight="semibold">{t("cv.toolbar.draft", { n: draft.n })}</Text>
            </Tagged>
            <Tagged testId="cv-status">
              <Label hue={draft.status === "approved" ? "green" : "blue"}>
                {draft.status === "approved"
                  ? t("cv.toolbar.approvedPill", { version: draft.version ?? 1 })
                  : t("cv.toolbar.draftPill")}
              </Label>
            </Tagged>
            <Tagged testId="cv-language">
              <Text tone="secondary" size="sm">
                {t("cv.toolbar.language", {
                  language: languageName(draft.language, i18n.language),
                })}
              </Text>
            </Tagged>
            <Button
              variant="secondary"
              ai
              disabled={busy || !cv.can_draft.allowed}
              data-testid="cv-make-again"
              onClick={() => {
                resetErrors();
                again.mutate();
              }}
            >
              {again.isPending
                ? t("cv.toolbar.making")
                : t("cv.toolbar.makeAgain", { cost: formatCost(cv.draft_eur, i18n.language) })}
            </Button>
          </Cluster>
        </Tagged>
        {draft.needs_version ? (
          <Tagged testId="cv-needs-version">
            <Banner
              icon="warning"
              title={t("cv.needsVersion.title")}
              action={
                <Button
                  variant="primary"
                  disabled={busy}
                  data-testid="cv-make-pdf"
                  onClick={() => {
                    resetErrors();
                    makeVersion.mutate(DEFAULT_LAYOUT);
                  }}
                >
                  {makeVersion.isPending ? t("cv.layout.making") : t("cv.needsVersion.make")}
                </Button>
              }
            >
              {t("cv.needsVersion.body")}
            </Banner>
          </Tagged>
        ) : null}
        {!cv.can_draft.allowed &&
        cv.can_draft.reason !== undefined &&
        cv.can_draft.reason !== null ? (
          <Tagged testId="cv-make-again-reason">
            <Text tone="secondary" size="sm">
              {t(`cv.draft.blocked.${cv.can_draft.reason}`)}
            </Text>
          </Tagged>
        ) : null}
        {failure === undefined ? null : (
          <Tagged testId="cv-error">
            <Banner
              tone="danger"
              icon="warning"
              role="alert"
              title={t(`cv.errors.${cvErrorCode(failure)}`, { count: failedLength })}
            >
              {pdfFailureReasons(failure).length === 0 ? null : (
                <Tagged testId="cv-error-reasons">
                  <Bullets items={pdfFailureReasons(failure)} />
                </Tagged>
              )}
            </Banner>
          </Tagged>
        )}
        {onLetter === undefined || newest === undefined ? null : (
          <Tagged testId="cv-letter">
            <Section title={t("letter.fromCv.title")} description={t("letter.fromCv.body")}>
              <LetterSlot
                id={id}
                testIdPrefix="cv-letter"
                onReview={() => {
                  onLetter(id);
                }}
              />
            </Section>
          </Tagged>
        )}
        <LayoutControls
          layout={layout}
          onLayout={(next) => {
            resetErrors();
            if (editable) {
              preview.mutate(next);
            } else {
              setAsked(next);
            }
          }}
          language={language}
          onLanguage={(next) => {
            setLanguage(cvLanguage(next));
          }}
          disabled={busy}
        />
        <ReviewLayout
          testId="cv-layout"
          aside={
            <ReviewAside
              cv={cv}
              draft={draft}
              spend={spend}
              actions={actions}
              onBack={onPipeline}
              onOpenPdf={setViewing}
            />
          }
        >
          <Stack>
            {editable ? (
              <>
                <PreviewPane
                  opportunity={id}
                  n={draft.n}
                  language={draft.language}
                  preview={draft.preview}
                />
                <Tagged testId="cv-lines">
                  <Disclosure
                    summary={
                      <Tagged testId="cv-lines-toggle">
                        <span>{t("cv.preview.lines")}</span>
                      </Tagged>
                    }
                  >
                    {sheet}
                  </Disclosure>
                </Tagged>
              </>
            ) : (
              sheet
            )}
            <VersionsSection opportunity={id} versions={cv.versions} onOpen={setViewing} />
          </Stack>
        </ReviewLayout>
        {editable ? (
          <ActionDock
            testId="cv-dock"
            actions={
              <Button
                variant="primary"
                icon="check"
                disabled={busy || !draft.can_approve}
                data-testid="cv-approve"
                onClick={() => {
                  resetErrors();
                  approve.mutate(layout);
                }}
              >
                {approve.isPending
                  ? t("cv.toolbar.approving")
                  : t("cv.toolbar.approve", { version })}
              </Button>
            }
            note={
              <Tagged testId={draft.can_approve ? "cv-approve-ready" : "cv-approve-reason"}>
                <Text tone="secondary" size="sm">
                  {draft.can_approve
                    ? t("cv.waiting.ready")
                    : t("cv.waiting.answer", { count: Math.max(waiting, 1) })}
                </Text>
              </Tagged>
            }
          />
        ) : undoing ? (
          <ActionDock
            testId="cv-dock"
            actions={
              <>
                <Tagged testId="cv-approved-now">
                  <Text weight="semibold">{t("cv.dock.approved")}</Text>
                </Tagged>
                <Button
                  variant="secondary"
                  disabled={busy}
                  data-testid="cv-undo"
                  onClick={() => {
                    resetErrors();
                    unapprove.mutate();
                  }}
                >
                  {unapprove.isPending ? t("cv.dock.undoing") : t("cv.dock.undo")}
                </Button>
              </>
            }
            note={
              cv.next === undefined || cv.next === null ? undefined : (
                <Tagged testId="cv-next">
                  <Text tone="secondary" size="sm">
                    {t("cv.dock.next", { cost: formatCost(cv.next.eur, i18n.language) })}
                  </Text>
                </Tagged>
              )
            }
          />
        ) : null}
        <Dialog
          open={asked !== null}
          onClose={() => {
            setAsked(null);
          }}
          visor={null}
          title={t("cv.layout.confirmTitle")}
          description={
            asked === null
              ? undefined
              : t("cv.layout.confirmBody", {
                  version: newest?.version ?? 0,
                  next: (newest?.version ?? 0) + 1,
                  template: t(`cv.layout.templates.${asked.template}`),
                  pages: t("cv.layout.pages", { count: asked.length }),
                })
          }
          footer={
            <>
              <Button
                variant="secondary"
                data-testid="cv-layout-cancel"
                onClick={() => {
                  setAsked(null);
                }}
              >
                {t("cv.layout.cancel")}
              </Button>
              <Button
                variant="primary"
                disabled={makeVersion.isPending}
                data-testid="cv-layout-confirm"
                onClick={() => {
                  if (asked !== null) {
                    makeVersion.mutate(asked, {
                      onSettled: () => {
                        setAsked(null);
                      },
                    });
                  }
                }}
              >
                {makeVersion.isPending
                  ? t("cv.layout.making")
                  : t("cv.layout.confirm", { next: (newest?.version ?? 0) + 1 })}
              </Button>
            </>
          }
        />
        <PdfViewer
          open={viewed !== undefined}
          onClose={() => {
            setViewing(null);
          }}
          url={cvPdfUrl(id, viewed?.version ?? 0)}
          downloadUrl={cvPdfUrl(id, viewed?.version ?? 0, true)}
          title={t("cv.viewer.title", { version: viewed?.version ?? 0 })}
          subtitle={
            viewed === undefined
              ? undefined
              : t("cv.viewer.subtitle", {
                  date: formatShortDate(viewed.created_at, i18n.language),
                  template: t(`cv.layout.templates.${viewed.template}`),
                  pages: t("cv.layout.pages", { count: viewed.pages }),
                  size: formatSize(viewed.size_bytes, i18n.language),
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
    </TouchTargets>
  );
}

/**
 * Whether the moment to undo an approval lasts: until `until`, then not
 * (the page is drawn again when it ends).
 */
function useUndoWindow(until: string | undefined): boolean {
  const end = until === undefined ? 0 : Date.parse(until);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const left = end - Date.now();
    if (left <= 0) {
      return;
    }
    const timer = setTimeout(() => {
      setNow(Date.now());
    }, left);
    return () => {
      clearTimeout(timer);
    };
  }, [end]);
  return end > now;
}
