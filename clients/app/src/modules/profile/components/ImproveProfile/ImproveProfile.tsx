import { useState, type KeyboardEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import { useShortcut } from "~/lib/shortcuts";
import {
  Banner,
  Bullets,
  Button,
  Cluster,
  FocusPage,
  Progress,
  Stack,
  Tag,
  Tagged,
  Text,
  TextArea,
} from "~/lib/ui";

import {
  useDiscardDraft,
  useDraftAnswer,
  useKeepDraft,
  useQuestions,
  useReviewAgain,
  useSkipQuestion,
  yearOf,
  type DraftView,
  type Question,
  type QuestionsView,
} from "../../api/completion";

/** Longest answer the server takes. */
const MAX_ANSWER = 2000;

export interface ImproveProfileProps {
  /** The question asked for (`?q=`); the most useful open one when none. */
  question: string | undefined;
  /** Leave: `later` (Finish later, Esc) or `done` (nothing left to ask). */
  onLeave: (reason: "later" | "done") => void;
}

/** The leave button: Finish later, Esc. */
function LeaveButton({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation("profile");
  return (
    <Button
      variant="ghost"
      icon="close"
      shortcut="overlay.close"
      data-testid="improve-later"
      onClick={onClick}
    >
      {t("improve.later")}
    </Button>
  );
}

/** What the question is about, as a short label. */
function useDescribeQuestion() {
  const { t } = useTranslation("profile");
  return (question: Question): string => {
    const role = question.context;
    if (question.section === "work" && role !== null && role !== undefined) {
      const title = [role.position, role.company]
        .filter((part): part is string => part !== null && part !== undefined && part !== "")
        .join(" · ");
      const from = yearOf(role.start_date);
      const to = yearOf(role.end_date);
      const years =
        from === undefined
          ? undefined
          : to === undefined
            ? t("improve.yearsNow", { from })
            : t("improve.years", { from, to });
      return t("improve.contextRole", { role: [title, years].filter(Boolean).join(" · ") });
    }
    switch (question.section) {
      case "basics.label": {
        return t("improve.context.label");
      }
      case "basics.summary": {
        return t("improve.context.summary");
      }
      case "education": {
        return t("improve.context.education");
      }
      default: {
        return t("improve.context.links");
      }
    }
  };
}

const ERROR_CODES = [
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
  "profile_changed",
  "draft_not_ready",
  "invalid_answer",
  "invalid_items",
  "not_found",
] as const;

/** The problem's code if the messages know it, else `generic`. */
function errorCode(error: unknown): (typeof ERROR_CODES)[number] | "generic" {
  const code = problemCode(error);
  return ERROR_CODES.find((known) => known === code) ?? "generic";
}

const REASONS = [
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
] as const;

/** Why the review could not be made, if the messages know it, else `ai_failed`. */
function reasonCode(reason: string | null | undefined): (typeof REASONS)[number] {
  return REASONS.find((known) => known === reason) ?? "ai_failed";
}

interface StepProps {
  view: QuestionsView;
  question: Question;
  kept: boolean;
  onKept: () => void;
  onLeave: (reason: "later" | "done") => void;
}

/**
 * One question on its own screen: the answer box, Draft from my answer, the
 * draft to keep, edit or discard, Skip, Finish later. Nothing is required and
 * nothing is saved before Keep.
 */
function Step({ view, question, kept, onKept, onLeave }: StepProps) {
  const { t } = useTranslation("profile");
  const describe = useDescribeQuestion();
  const draftAnswer = useDraftAnswer();
  const keepDraft = useKeepDraft();
  const discardDraft = useDiscardDraft();
  const skip = useSkipQuestion();
  const [answer, setAnswer] = useState("");
  const [draft, setDraft] = useState<DraftView | null>(null);
  const [editing, setEditing] = useState<string | null>(null);

  const later = () => {
    onLeave("later");
  };

  const busy = draftAnswer.isPending || keepDraft.isPending || skip.isPending;
  const makeDraft = () => {
    const text = answer.trim();
    if (text === "" || busy) {
      return;
    }
    keepDraft.reset();
    draftAnswer.mutate(
      { id: question.id, answer: text },
      {
        onSuccess: (made) => {
          setDraft(made);
          setEditing(null);
        },
      },
    );
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      makeDraft();
    }
  };
  const keep = () => {
    if (draft === null) {
      return;
    }
    const edited =
      editing === null
        ? {}
        : {
            items: editing
              .split("\n")
              .map((line) => line.trim())
              .filter((line) => line !== ""),
          };
    keepDraft.mutate({ id: draft.id, ...edited }, { onSuccess: onKept });
  };
  const discard = () => {
    if (draft !== null) {
      discardDraft.mutate(draft.id);
    }
    setDraft(null);
    setEditing(null);
    keepDraft.reset();
  };

  const position = Math.min(view.answered + 1, view.total);
  const eyebrow = t("improve.eyebrow", { n: position, total: view.total });
  const gain = Math.min(100, view.score + question.expected_gain);
  const canEdit = draft !== null && draft.kind !== "education";

  return (
    <Tagged testId="improve-question">
      <FocusPage
        top={<LeaveButton onClick={later} />}
        eyebrow={eyebrow}
        title={question.question}
        lead={question.why}
        formLabel={question.question}
        onSubmit={(event) => {
          event.preventDefault();
          makeDraft();
        }}
        actions={
          <>
            <Button
              variant="primary"
              size="lg"
              type="submit"
              icon="arrow-right"
              disabled={busy || answer.trim() === ""}
              data-testid="improve-draft"
            >
              {t(draftAnswer.isPending ? "improve.drafting" : "improve.draft")}
            </Button>
            <Button
              variant="ghost"
              size="lg"
              disabled={busy}
              data-testid="improve-skip"
              onClick={() => {
                skip.mutate(question.id);
              }}
            >
              {t("improve.skip")}
            </Button>
          </>
        }
      >
        <Stack gap="md">
          <Tagged testId="improve-progress">
            <Progress label={eyebrow} value={position} max={view.total}>
              {t("improve.strengthNow", { from: view.score, to: gain })}
            </Progress>
          </Tagged>
          {kept ? (
            <Tagged testId="improve-kept">
              <Banner icon="check" role="status" title={t("improve.kept", { score: view.score })} />
            </Tagged>
          ) : null}
          <Tagged testId="improve-context">
            <Tag icon="tasks">{describe(question)}</Tag>
          </Tagged>
          <TextArea
            label={t("improve.answerLabel")}
            hint={t("improve.answerHint")}
            placeholder={t("improve.answerPlaceholder")}
            rows={5}
            maxLength={MAX_ANSWER}
            value={answer}
            data-testid="improve-answer"
            onChange={(event) => {
              setAnswer(event.target.value);
            }}
            onKeyDown={onKeyDown}
          />
          {draftAnswer.isError ? (
            <Tagged testId="improve-draft-error">
              <Banner
                tone="danger"
                icon="warning"
                role="alert"
                title={t(`improve.errors.${errorCode(draftAnswer.error)}`)}
              />
            </Tagged>
          ) : null}
          {skip.isError ? (
            <Tagged testId="improve-skip-error">
              <Banner
                tone="danger"
                icon="warning"
                role="alert"
                title={t(`improve.errors.${errorCode(skip.error)}`)}
              />
            </Tagged>
          ) : null}
          {draft === null ? null : (
            <Tagged testId="improve-draft-block">
              <Banner icon="edit" title={t("improve.draftTitle")}>
                <Stack gap="sm">
                  {editing === null ? (
                    draft.items.length === 0 ? (
                      <Tagged testId="improve-draft-empty">
                        <Text>{t("improve.nothingDrafted")}</Text>
                      </Tagged>
                    ) : (
                      <Bullets items={draft.items} testId="improve-draft-items" />
                    )
                  ) : (
                    <TextArea
                      label={t("improve.editLabel")}
                      rows={4}
                      value={editing}
                      data-testid="improve-draft-edit"
                      onChange={(event) => {
                        setEditing(event.target.value);
                      }}
                    />
                  )}
                  <Text tone="tertiary" size="sm">
                    {t("improve.draftSource")}
                  </Text>
                  {draft.dropped > 0 ? (
                    <Tagged testId="improve-dropped">
                      <Text tone="secondary" size="sm">
                        {t("improve.dropped", { count: draft.dropped })}
                      </Text>
                    </Tagged>
                  ) : null}
                  <Cluster>
                    <Button
                      variant="primary"
                      disabled={busy || draft.items.length === 0}
                      data-testid="improve-keep"
                      onClick={keep}
                    >
                      {t("improve.keep")}
                    </Button>
                    {canEdit && editing === null ? (
                      <Button
                        variant="secondary"
                        disabled={busy || draft.items.length === 0}
                        data-testid="improve-edit"
                        onClick={() => {
                          setEditing(draft.items.join("\n"));
                        }}
                      >
                        {t("improve.edit")}
                      </Button>
                    ) : null}
                    <Button
                      variant="ghost"
                      disabled={busy}
                      data-testid="improve-discard"
                      onClick={discard}
                    >
                      {t("improve.discard")}
                    </Button>
                  </Cluster>
                </Stack>
              </Banner>
            </Tagged>
          )}
          {keepDraft.isError ? (
            <Tagged testId="improve-keep-error">
              <Banner
                tone="danger"
                icon="warning"
                role="alert"
                title={t(`improve.errors.${errorCode(keepDraft.error)}`)}
              />
            </Tagged>
          ) : null}
        </Stack>
      </FocusPage>
    </Tagged>
  );
}

/** The page when the questions are not there: why, and the way out. */
function Message({
  testId,
  title,
  lead,
  actions,
  onLeave,
}: {
  testId: string;
  title: string;
  lead?: string | undefined;
  actions?: ReactNode;
  onLeave: (reason: "later" | "done") => void;
}) {
  return (
    <Tagged testId={testId}>
      <FocusPage
        title={title}
        lead={lead}
        top={
          <LeaveButton
            onClick={() => {
              onLeave("later");
            }}
          />
        }
        actions={actions}
      />
    </Tagged>
  );
}

/**
 * Improve my profile: the AI's questions about what is missing or thin, one
 * per screen, each skippable, never forced. An answer becomes a draft made
 * only of the person's own words and facts; nothing is saved until they keep
 * it. Esc leaves, keeping what was kept.
 */
export function ImproveProfile({ question, onLeave }: ImproveProfileProps) {
  const { t } = useTranslation("profile");
  const questions = useQuestions();
  const again = useReviewAgain();
  const [kept, setKept] = useState(false);
  useShortcut("overlay.close", () => {
    onLeave("later");
  });

  if (questions.isPending || questions.data?.status === "running") {
    return (
      <Tagged testId="improve-reading">
        <FocusPage
          title={t("improve.readingTitle")}
          lead={t("improve.readingLead")}
          top={
            <LeaveButton
              onClick={() => {
                onLeave("later");
              }}
            />
          }
        >
          <Progress label={t("improve.readingTitle")} value={0} max={0} />
        </FocusPage>
      </Tagged>
    );
  }
  if (questions.isError) {
    return (
      <Message
        testId="improve-load-error"
        title={t("improve.loadFailed")}
        onLeave={onLeave}
        actions={
          <Button
            variant="primary"
            data-testid="improve-retry"
            onClick={() => {
              void questions.refetch();
            }}
          >
            {t("retry")}
          </Button>
        }
      />
    );
  }
  const view = questions.data;
  if (view.status === "unavailable") {
    return (
      <Message
        testId="improve-unavailable"
        title={t("improve.unavailableTitle")}
        lead={t(
          view.reason === undefined || view.reason === null
            ? "improve.noProfile"
            : "improve.unavailableLead",
        )}
        onLeave={onLeave}
        actions={
          <Button
            variant="primary"
            data-testid="improve-back"
            onClick={() => {
              onLeave("done");
            }}
          >
            {t("improve.backToProfile")}
          </Button>
        }
      />
    );
  }
  if (view.status === "failed") {
    return (
      <Message
        testId="improve-failed"
        title={t("improve.failedTitle")}
        lead={t(`improve.reasons.${reasonCode(view.reason)}`)}
        onLeave={onLeave}
        actions={
          <>
            <Button
              variant="primary"
              disabled={again.isPending}
              data-testid="improve-review-again"
              onClick={() => {
                again.mutate();
              }}
            >
              {t("retry")}
            </Button>
            <Button
              variant="ghost"
              data-testid="improve-back"
              onClick={() => {
                onLeave("done");
              }}
            >
              {t("improve.backToProfile")}
            </Button>
          </>
        }
      />
    );
  }
  const current = view.questions.find((q) => q.id === question) ?? view.questions[0];
  if (current === undefined) {
    return (
      <Message
        testId="improve-done"
        title={t("improve.doneTitle")}
        lead={t("improve.doneLead", { score: view.score })}
        onLeave={onLeave}
        actions={
          <Button
            variant="primary"
            icon="arrow-right"
            data-testid="improve-back"
            onClick={() => {
              onLeave("done");
            }}
          >
            {t("improve.backToProfile")}
          </Button>
        }
      />
    );
  }
  return (
    <Step
      key={current.id}
      view={view}
      question={current}
      kept={kept}
      onKept={() => {
        setKept(true);
      }}
      onLeave={onLeave}
    />
  );
}
