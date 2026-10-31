import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import { Banner, Button, Cluster, Label, Page, ReviewLayout, Stack, Tagged, Text } from "~/lib/ui";

import {
  cvErrorCode,
  useApprove,
  useChangeLine,
  useConfirmLine,
  useCv,
  useMakeDraft,
  usePutBack,
  type Cv,
  type CvDraft,
} from "../../api/cv";
import { formatCost } from "../../api/labels";
import { useOpportunity } from "../../api/opportunities";
import { titleOf } from "../PipelinePage/OpportunityViews";
import { CvSheet } from "./CvSheet";
import { DraftSlot } from "./DraftSlot";
import type { LineActions } from "./LineRow";
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
 * `/pipeline/$id/cv`: the review of a tailored CV. The CV as a page, every
 * line marked by how it relates to the profile and, when it can't be traced
 * to it, blocked with the reason and what to do; beside it the check, the
 * job's keywords and what changed. Approve is off while any line is blocked.
 */
export function ReviewPage({ id, onBack, onPipeline, spend }: ReviewPageProps) {
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
}: {
  id: string;
  cv: Cv;
  draft: CvDraft;
  spend: ReactNode;
  onPipeline: () => void;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const change = useChangeLine(id, draft.n);
  const confirm = useConfirmLine(id, draft.n);
  const putBack = usePutBack(id, draft.n);
  const approve = useApprove(id, draft.n);
  const again = useMakeDraft(id);
  const editable = draft.status === "draft";
  const busy =
    change.isPending ||
    confirm.isPending ||
    putBack.isPending ||
    approve.isPending ||
    again.isPending;
  const failure = [change, confirm, putBack, approve, again].find(
    (action) => action.isError,
  )?.error;

  const resetErrors = () => {
    for (const action of [change, confirm, putBack, approve, again]) {
      action.reset();
    }
  };
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
  };
  const version = draft.version ?? (cv.versions[0]?.version ?? 0) + 1;
  const goFirst = () => {
    if (draft.first_flagged === undefined || draft.first_flagged === null) {
      return;
    }
    const target = document.getElementById(`cv-line-${draft.first_flagged}`);
    target?.scrollIntoView({ behavior: "smooth", block: "center" });
    target?.querySelector("button")?.focus({ preventScroll: true });
  };

  return (
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
              {t("cv.toolbar.language", { language: languageName(draft.language, i18n.language) })}
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
          {editable ? (
            <Button
              variant="primary"
              icon="check"
              disabled={busy || !draft.can_approve}
              data-testid="cv-approve"
              onClick={() => {
                resetErrors();
                approve.mutate();
              }}
            >
              {approve.isPending ? t("cv.toolbar.approving") : t("cv.toolbar.approve", { version })}
            </Button>
          ) : null}
        </Cluster>
      </Tagged>
      {editable && !draft.can_approve ? (
        <Tagged testId="cv-approve-reason">
          <Text tone="secondary" size="sm">
            {t("cv.toolbar.approveWhy")}
          </Text>
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
            title={t(`cv.errors.${cvErrorCode(failure)}`)}
          />
        </Tagged>
      )}
      <ReviewLayout
        testId="cv-layout"
        aside={
          <ReviewAside cv={cv} draft={draft} spend={spend} onFirst={goFirst} onBack={onPipeline} />
        }
      >
        <CvSheet
          draft={draft}
          actions={actions}
          onPutBack={(what) => {
            resetErrors();
            putBack.mutate(what);
          }}
        />
      </ReviewLayout>
    </Stack>
  );
}
