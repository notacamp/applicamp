import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Stack, Tagged, Text, TextArea } from "~/lib/ui";

import {
  prepareErrorCode,
  useAnswerQuestion,
  useDontHave,
  useSkipQuestion,
  type AnswerResult,
  type Brief,
  type BriefQuestion,
  type RoleRef,
} from "../../api/brief";
import { formatCost } from "../../api/labels";

/** Longest answer the server takes. */
export const MAX_ANSWER = 2000;

/** "Senior Engineer · Example Org". */
export function roleName(role: RoleRef): string {
  return [role.title, role.company]
    .filter((part): part is string => part !== undefined && part !== null && part !== "")
    .join(" · ");
}

export interface QuestionsProps {
  id: string;
  brief: Brief;
  /** The facts taken from an answer, with Keep, Fix and Drop (the profile's). */
  renderFacts: (answer: AnswerResult, role: string) => ReactNode;
}

/**
 * The questions for this job: asked only where the profile is thin for what
 * the job wants. Answer (one small model call, the facts wait for Keep, Fix
 * or Drop on a position of the profile), "I don't have it" (for a skill: no
 * call, never asked about again) and Skip (hidden for this job).
 */
export function Questions({ id, brief, renderFacts }: QuestionsProps) {
  const [answers, setAnswers] = useState<Record<string, AnswerResult>>({});
  const shown = brief.questions.filter(
    (question) =>
      question.state === "open" || (question.state === "answered" && question.id in answers),
  );
  if (shown.length === 0) {
    return null;
  }
  return (
    <Stack>
      {shown.map((question) => {
        const answered = answers[question.id];
        return answered === undefined ? (
          <QuestionCard
            key={question.id}
            id={id}
            brief={brief}
            question={question}
            onAnswered={(result) => {
              setAnswers((all) => ({ ...all, [question.id]: result }));
            }}
          />
        ) : (
          <Answered
            key={question.id}
            question={question}
            answer={answered}
            renderFacts={renderFacts}
          />
        );
      })}
    </Stack>
  );
}

function Answered({
  question,
  answer,
  renderFacts,
}: {
  question: BriefQuestion;
  answer: AnswerResult;
  renderFacts: QuestionsProps["renderFacts"];
}) {
  const { t } = useTranslation("pipeline");
  const role = roleName(answer.role);
  return (
    <Tagged testId={`prepare-answered-${question.id}`}>
      <Stack gap="sm">
        <Text>{question.question}</Text>
        <Text tone="secondary" size="sm">
          {answer.defaulted
            ? t("prepare.questions.wentToLatest", { role })
            : t("prepare.questions.wentTo", { role })}
        </Text>
        {answer.proposals.length === 0 ? (
          <Tagged testId={`prepare-answer-nothing-${question.id}`}>
            <Text tone="secondary">{t("prepare.questions.nothing")}</Text>
          </Tagged>
        ) : (
          renderFacts(answer, role)
        )}
        {answer.dropped > 0 ? (
          <Text tone="secondary" size="sm">
            {t("prepare.questions.dropped", { count: answer.dropped })}
          </Text>
        ) : null}
      </Stack>
    </Tagged>
  );
}

function QuestionCard({
  id,
  brief,
  question,
  onAnswered,
}: {
  id: string;
  brief: Brief;
  question: BriefQuestion;
  onAnswered: (answer: AnswerResult) => void;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const answer = useAnswerQuestion(id, i18n.language);
  const dontHave = useDontHave(id);
  const skip = useSkipQuestion(id);
  const [text, setText] = useState("");
  const busy = answer.isPending || dontHave.isPending || skip.isPending;
  const failure = [answer, dontHave, skip].find((action) => action.isError)?.error;
  const goesTo = question.about ?? brief.latest_role ?? undefined;
  const defaulted = question.about === undefined || question.about === null;
  const skill = question.skill ?? undefined;

  return (
    <Tagged testId={`prepare-question-${question.id}`}>
      <Stack gap="sm">
        <Text>{question.question}</Text>
        {goesTo === undefined ? (
          <Tagged testId={`prepare-question-goes-${question.id}`}>
            <Text tone="secondary" size="sm">
              {t("prepare.questions.noPosition")}
            </Text>
          </Tagged>
        ) : (
          <Tagged testId={`prepare-question-goes-${question.id}`}>
            <Text tone="secondary" size="sm">
              {defaulted
                ? t("prepare.questions.goesToLatest", { role: roleName(goesTo) })
                : t("prepare.questions.goesTo", { role: roleName(goesTo) })}
            </Text>
          </Tagged>
        )}
        <TextArea
          label={t("prepare.questions.yourAnswer")}
          hint={t("prepare.questions.cost", {
            cost: formatCost(brief.estimates.answer_eur, i18n.language),
          })}
          rows={3}
          maxLength={MAX_ANSWER}
          value={text}
          onChange={(event) => {
            setText(event.target.value);
          }}
          data-testid={`prepare-answer-${question.id}`}
        />
        <Cluster>
          <Button
            variant="primary"
            icon="arrow-right"
            disabled={busy || text.trim() === "" || goesTo === undefined}
            data-testid={`prepare-answer-send-${question.id}`}
            onClick={() => {
              answer.reset();
              answer.mutate(
                { question: question.id, text: text.trim() },
                { onSuccess: onAnswered },
              );
            }}
          >
            {answer.isPending ? t("prepare.questions.answering") : t("prepare.questions.answer")}
          </Button>
          {skill === undefined ? null : (
            <Button
              variant="secondary"
              disabled={busy}
              data-testid={`prepare-donthave-${question.id}`}
              onClick={() => {
                dontHave.mutate(question.id);
              }}
            >
              {t("prepare.questions.dontHave")}
            </Button>
          )}
          <Button
            variant="ghost"
            disabled={busy}
            data-testid={`prepare-skip-${question.id}`}
            onClick={() => {
              skip.mutate(question.id);
            }}
          >
            {t("prepare.questions.skip")}
          </Button>
        </Cluster>
        {skill === undefined ? null : (
          <Text tone="secondary" size="sm">
            {t("prepare.questions.dontHaveHint", { skill })}
          </Text>
        )}
        {failure === undefined ? null : (
          <Tagged testId={`prepare-question-error-${question.id}`}>
            <Banner
              tone="danger"
              icon="warning"
              role="alert"
              title={t(`prepare.errors.${prepareErrorCode(failure)}`)}
            />
          </Tagged>
        )}
      </Stack>
    </Tagged>
  );
}
