import { useState } from "react";
import { useTranslation } from "react-i18next";

import { useShortcut } from "~/lib/shortcuts";
import { Banner, Button, FocusPage, Progress, Stack, Tagged, type FocusPageProps } from "~/lib/ui";

import { useGoals, useSaveGoals, type Goals } from "../../api/goals";
import {
  neighbour,
  pendingOf,
  QUESTIONS,
  questionsFor,
  sameAnswers,
  settle,
  type Question,
} from "../../api/questions";
import { GoalQuestion } from "../GoalQuestion/GoalQuestion";

export interface GoalInterviewProps {
  /** The question on screen (`?step=`); the first when none is asked for. */
  question: Question | undefined;
  /** Go to another question. */
  onQuestion: (question: Question) => void;
  /** Leave: `later` (Finish later, Esc) or `done` (past the last question). */
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
      data-testid="interview-later"
      onClick={onClick}
    >
      {t("goals.interview.later")}
    </Button>
  );
}

interface StepProps {
  question: Question;
  /** The answers as saved. */
  answers: Goals;
  onQuestion: (question: Question) => void;
  onLeave: (reason: "later" | "done") => void;
}

/**
 * One question on its own screen: Next saves the answer and moves on, Skip
 * moves on without touching it, Back goes to the one before, Finish later
 * (or Esc) leaves with what is answered saved. Nothing is ever required.
 */
function Step({ question, answers, onQuestion, onLeave }: StepProps) {
  const { t } = useTranslation("profile");
  const save = useSaveGoals();
  const [working, setWorking] = useState<Goals>(answers);
  const [pending, setPending] = useState(() => pendingOf(answers));
  const [amountInvalid, setAmountInvalid] = useState(false);

  const asked = questionsFor(answers);
  const position = Math.max(asked.indexOf(question), 0) + 1;
  const previous = neighbour(answers, question, "previous");
  const isLast = neighbour(answers, question, "next") === undefined;

  const moveOn = (from: Goals) => {
    const to = neighbour(from, question, "next");
    if (to === undefined) {
      onLeave("done");
    } else {
      onQuestion(to);
    }
  };
  const next = () => {
    const result = settle(question, working, pending);
    if (result === "invalid") {
      setAmountInvalid(true);
      return;
    }
    if (sameAnswers(result, answers)) {
      moveOn(answers);
      return;
    }
    save.mutate(result, {
      onSuccess: (view) => {
        moveOn(view.answers);
      },
    });
  };
  const later = () => {
    const result = settle(question, working, pending);
    if (result === "invalid" || sameAnswers(result, answers)) {
      onLeave("later");
      return;
    }
    save.mutate(result, {
      onSuccess: () => {
        onLeave("later");
      },
    });
  };
  useShortcut("overlay.close", later);

  const submit: FocusPageProps["onSubmit"] = (event) => {
    event.preventDefault();
    next();
  };

  return (
    <Tagged testId={`interview-${question}`}>
      <FocusPage
        top={<LeaveButton onClick={later} />}
        eyebrow={t("goals.interview.progress", { n: position, total: asked.length })}
        title={t(`goals.q.${question}.title`)}
        lead={`${t(`goals.q.${question}.lead`)} ${t("goals.interview.optional")}`}
        onSubmit={submit}
        formLabel={t(`goals.q.${question}.title`)}
        actions={
          <>
            <Button
              variant="primary"
              size="lg"
              type="submit"
              keys="enter"
              icon="arrow-right"
              disabled={save.isPending}
              data-testid="interview-next"
            >
              {t(isLast ? "goals.interview.done" : "goals.interview.next")}
            </Button>
            <Button
              variant="ghost"
              size="lg"
              disabled={save.isPending}
              data-testid="interview-skip"
              onClick={() => {
                moveOn(answers);
              }}
            >
              {t("goals.interview.skip")}
            </Button>
            {previous === undefined ? null : (
              <Button
                variant="ghost"
                size="lg"
                icon="chevron-left"
                disabled={save.isPending}
                data-testid="interview-back"
                onClick={() => {
                  onQuestion(previous);
                }}
              >
                {t("goals.interview.back")}
              </Button>
            )}
          </>
        }
      >
        <Stack gap="md">
          <Tagged testId="interview-progress">
            <Progress
              label={t("goals.interview.progress", { n: position, total: asked.length })}
              value={position}
              max={asked.length}
            />
          </Tagged>
          <GoalQuestion
            question={question}
            answers={working}
            onAnswers={setWorking}
            pending={pending}
            onPending={(next) => {
              setPending(next);
              setAmountInvalid(false);
            }}
            amountInvalid={amountInvalid}
          />
          {save.isError ? (
            <Tagged testId="interview-save-error">
              <Banner
                tone="danger"
                icon="warning"
                role="alert"
                title={t("goals.interview.saveFailed")}
              />
            </Tagged>
          ) : null}
        </Stack>
      </FocusPage>
    </Tagged>
  );
}

/**
 * The goals interview: one question per screen, every one skippable, never
 * forced. Each Next saves the answers so far, so leaving at any point keeps
 * them.
 */
export function GoalInterview({ question, onQuestion, onLeave }: GoalInterviewProps) {
  const { t } = useTranslation("profile");
  const goals = useGoals();

  if (goals.isPending) {
    return (
      <Tagged testId="interview-loading">
        <FocusPage title={t("goals.interview.loading")} />
      </Tagged>
    );
  }
  if (goals.isError) {
    return (
      <Tagged testId="interview-load-error">
        <FocusPage
          title={t("goals.loadFailed")}
          actions={
            <>
              <Button
                variant="primary"
                data-testid="interview-retry"
                onClick={() => {
                  void goals.refetch();
                }}
              >
                {t("retry")}
              </Button>
              <LeaveButton
                onClick={() => {
                  onLeave("later");
                }}
              />
            </>
          }
        />
      </Tagged>
    );
  }
  const current = question ?? QUESTIONS[0];
  return (
    <Step
      key={current}
      question={current}
      answers={goals.data.answers}
      onQuestion={onQuestion}
      onLeave={onLeave}
    />
  );
}
