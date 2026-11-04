import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import {
  Banner,
  Button,
  Cluster,
  formatSize,
  Label,
  Page,
  PdfViewer,
  ReviewLayout,
  Select,
  Stack,
  Tagged,
  Text,
  ViewSwitch,
} from "~/lib/ui";

import { formatCost, formatShortDate, languageName } from "../../api/labels";
import { useOpportunity } from "../../api/opportunities";
import {
  letterErrorCode,
  letterPdfUrl,
  SIZES,
  useApproveLetter,
  useChangeAnswer,
  useChangeSentence,
  useConfirmSentence,
  useLetter,
  useWriteLetter,
  type Letter,
  type LetterDraft,
  type LetterSize,
} from "../../api/letter";
import { titleOf } from "../PipelinePage/OpportunityViews";
import { CV_LANGUAGES, cvLanguage } from "../Review/LayoutControls";
import { Answers, type AnswerActions } from "./Answers";
import { LetterAside } from "./LetterAside";
import { LetterSheet } from "./LetterSheet";
import { LetterSlot } from "./LetterSlot";
import type { SentenceActions } from "./SentenceFix";

export interface LetterPageProps {
  /** The opportunity. */
  id: string;
  /** Back to the CV. */
  onBack: () => void;
  /** Back to the pipeline (once approved). */
  onPipeline: () => void;
  /** The month's AI spend line. */
  spend?: ReactNode;
}

/**
 * `/pipeline/$id/letter`: the review of the cover letter. The letter as a
 * page, each paragraph tagged by what it rests on (a company fact, your
 * profile, no claims) and, when a sentence can't be traced, blocked with the
 * reason and what to do; below it the job's own questions with their answers;
 * beside it the check, "At a glance" and what it cost. Approve is off while
 * anything is open.
 */
export function LetterPage({ id, onBack, onPipeline, spend }: LetterPageProps) {
  const { t } = useTranslation("pipeline");
  const opportunity = useOpportunity(id);
  const letter = useLetter(id);
  const title =
    opportunity.data === undefined ? t("letter.title") : titleOf(opportunity.data, t("untitled"));
  const company = opportunity.data?.company ?? "";

  let body: ReactNode;
  if (letter.isPending || opportunity.isPending) {
    body = (
      <Tagged testId="letter-loading">
        <Text tone="secondary">{t("letter.loading")}</Text>
      </Tagged>
    );
  } else if (letter.isError || opportunity.isError) {
    const missing =
      problemCode(letter.error) === "not_found" || problemCode(opportunity.error) === "not_found";
    body = (
      <Tagged testId="letter-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t(missing ? "letter.errors.not_found" : "letter.loadFailed")}
          action={
            missing ? undefined : (
              <Button
                variant="secondary"
                onClick={() => {
                  void letter.refetch();
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
  } else if (letter.data.draft === undefined || letter.data.draft === null) {
    body = (
      <Tagged testId="letter-empty">
        <Stack>
          <Text tone="secondary">{t("letter.empty.body")}</Text>
          <LetterSlot id={id} testIdPrefix="letter" />
        </Stack>
      </Tagged>
    );
  } else {
    body = (
      <Review
        key={letter.data.draft.n}
        id={id}
        letter={letter.data}
        draft={letter.data.draft}
        spend={spend}
        onPipeline={onPipeline}
      />
    );
  }

  return (
    <Page
      testId="letter-review"
      wide
      eyebrow={t("letter.eyebrow")}
      title={company === "" ? title : t("letter.heading", { company })}
      lead={t("letter.lead")}
    >
      <Cluster>
        <Button variant="ghost" data-testid="letter-back" onClick={onBack}>
          {t("letter.back")}
        </Button>
      </Cluster>
      {body}
    </Page>
  );
}

function Review({
  id,
  letter,
  draft,
  spend,
  onPipeline,
}: {
  id: string;
  letter: Letter;
  draft: LetterDraft;
  spend: ReactNode;
  onPipeline: () => void;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const change = useChangeSentence(id, draft.n);
  const confirm = useConfirmSentence(id, draft.n);
  const answer = useChangeAnswer(id, draft.n);
  const approve = useApproveLetter(id, draft.n);
  const [size, setSize] = useState<LetterSize>(draft.size);
  const [language, setLanguage] = useState(() => cvLanguage(draft.language));
  const again = useWriteLetter(id, { language, size });
  const editable = draft.status === "draft";
  const [viewing, setViewing] = useState<number | null>(null);
  const actionsList = [change, confirm, answer, approve, again];
  const busy = actionsList.some((action) => action.isPending);
  const failure = actionsList.find((action) => action.isError)?.error;
  const resetErrors = () => {
    for (const action of actionsList) {
      action.reset();
    }
  };
  const viewed = letter.versions.find((version) => version.version === viewing);
  const sentences: SentenceActions = {
    editable,
    busy,
    edit: (sentence, text) => {
      resetErrors();
      change.mutate({ sentence, change: { text } });
    },
    remove: (sentence) => {
      resetErrors();
      change.mutate({ sentence, change: { remove: true } });
    },
    confirm: (sentence, text) => {
      resetErrors();
      confirm.mutate({ sentence, text });
    },
  };
  const answers: AnswerActions = {
    editable,
    busy,
    write: (target, text) => {
      resetErrors();
      answer.mutate({ answer: target, change: { text } });
    },
    remove: (target) => {
      resetErrors();
      answer.mutate({ answer: target, change: { remove: true } });
    },
  };
  const goFirst = () => {
    if (draft.first_flagged === undefined || draft.first_flagged === null) {
      return;
    }
    const target =
      document.querySelector(`[data-testid="letter-flag-${CSS.escape(draft.first_flagged)}"]`) ??
      document.querySelector(`[data-testid="letter-answer-${CSS.escape(draft.first_flagged)}"]`);
    target?.scrollIntoView({ behavior: "smooth", block: "center" });
    target?.closest("li")?.querySelector("button")?.focus({ preventScroll: true });
  };

  return (
    <Stack>
      <Tagged testId="letter-toolbar">
        <Cluster>
          <Tagged testId="letter-draft-number">
            <Text weight="semibold">{t("letter.toolbar.draft", { n: draft.n })}</Text>
          </Tagged>
          <Tagged testId="letter-status">
            <Label hue={draft.status === "approved" ? "green" : "blue"}>
              {draft.status === "approved"
                ? t("letter.toolbar.approvedPill", { version: draft.version ?? 1 })
                : t("letter.toolbar.draftPill")}
            </Label>
          </Tagged>
          <Tagged testId="letter-language">
            <Text tone="secondary" size="sm">
              {t("letter.toolbar.language", {
                language: languageName(draft.language, i18n.language),
              })}
            </Text>
          </Tagged>
          <Button
            variant="secondary"
            ai
            disabled={busy || !letter.can_write.allowed}
            data-testid="letter-make-again"
            onClick={() => {
              resetErrors();
              again.mutate();
            }}
          >
            {again.isPending ? t("letter.toolbar.making") : t("letter.toolbar.makeAgain")}
          </Button>
          {editable ? (
            <Button
              variant="primary"
              icon="check"
              disabled={busy || !draft.can_approve}
              data-testid="letter-approve"
              onClick={() => {
                resetErrors();
                approve.mutate();
              }}
            >
              {approve.isPending ? t("letter.toolbar.approving") : t("letter.toolbar.approve")}
            </Button>
          ) : null}
        </Cluster>
      </Tagged>
      <Tagged testId="letter-make-again-cost">
        <Text tone="secondary" size="sm">
          {t("letter.toolbar.cost", { cost: formatCost(letter.draft_eur, i18n.language) })}
        </Text>
      </Tagged>
      <Tagged testId="letter-controls">
        <Cluster>
          <Text tone="secondary" size="sm">
            {t("letter.toolbar.length")}
          </Text>
          <ViewSwitch
            label={t("letter.toolbar.length")}
            options={SIZES.map((value) => ({
              value,
              label: t(`letter.toolbar.sizes.${value}`),
              testId: `letter-size-${value}`,
            }))}
            value={size}
            onChange={(next) => {
              if (!busy) {
                setSize(next);
              }
            }}
          />
          <Select
            label={t("letter.toolbar.languageLabel")}
            options={CV_LANGUAGES.map((tag) => ({
              value: tag,
              label: languageName(tag, i18n.language),
            }))}
            value={cvLanguage(language)}
            onChange={(next) => {
              setLanguage(cvLanguage(next));
            }}
            data-testid="letter-language-select"
          />
          <Text tone="secondary" size="sm">
            {t("letter.toolbar.note")}
          </Text>
        </Cluster>
      </Tagged>
      {editable && !draft.can_approve ? (
        <Tagged testId="letter-approve-reason">
          <Text tone="secondary" size="sm">
            {t("letter.toolbar.approveWhy")}
          </Text>
        </Tagged>
      ) : null}
      {!letter.can_write.allowed &&
      letter.can_write.reason !== undefined &&
      letter.can_write.reason !== null ? (
        <Tagged testId="letter-make-again-reason">
          <Text tone="secondary" size="sm">
            {t(`letter.slot.blocked.${letter.can_write.reason}`)}
          </Text>
        </Tagged>
      ) : null}
      {failure === undefined ? null : (
        <Tagged testId="letter-error">
          <Banner
            tone="danger"
            icon="warning"
            role="alert"
            title={t(`letter.errors.${letterErrorCode(failure)}`)}
          />
        </Tagged>
      )}
      <ReviewLayout
        testId="letter-layout"
        aside={
          <LetterAside
            letter={letter}
            draft={draft}
            spend={spend}
            onFirst={goFirst}
            onBack={onPipeline}
            onOpenPdf={setViewing}
          />
        }
      >
        <Stack>
          <LetterSheet draft={draft} actions={sentences} />
          <Answers draft={draft} actions={answers} />
        </Stack>
      </ReviewLayout>
      <PdfViewer
        open={viewed !== undefined}
        onClose={() => {
          setViewing(null);
        }}
        url={letterPdfUrl(id, viewed?.version ?? 0)}
        downloadUrl={letterPdfUrl(id, viewed?.version ?? 0, true)}
        title={t("letter.viewer.title", { version: viewed?.version ?? 0 })}
        subtitle={
          viewed === undefined
            ? undefined
            : t("letter.viewer.subtitle", {
                date: formatShortDate(viewed.created_at, i18n.language),
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
  );
}
