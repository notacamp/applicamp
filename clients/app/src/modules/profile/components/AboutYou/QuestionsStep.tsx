import type { TFunction } from "i18next";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Chip,
  Cluster,
  Disclosure,
  FactBlock,
  FactRow,
  Heading,
  Stack,
  Tag,
  Tagged,
  Text,
  TextArea,
} from "~/lib/ui";

import {
  MAX_EXTRA,
  useAnswerQuestion,
  useEnoughQuestions,
  useNextQuestion,
  useSkipQuestion,
  type AboutChange,
  type AboutQuestion,
  type AboutView,
} from "../../api/about";
import { ChangeActions } from "./ChangeActions";
import { errorCode, firstError } from "./labels";
import { AboutLegend, PortraitText } from "./PortraitText";

/** The kinds of question whose answer can be several of the choices. */
const MANY: ReadonlySet<AboutQuestion["kind"]> = new Set(["assumed", "skills", "skills_found"]);

/** Where a change goes in the profile, in words. */
function whereOf(t: TFunction<"profile">, change: AboutChange, view: AboutView): string {
  switch (change.field) {
    case "summary":
    case "headline":
    case "background": {
      return t(`about.groups.${change.field}`);
    }
    case "skill": {
      return t("about.groups.skills");
    }
    case "role_fact": {
      const employer = view.employers.find((candidate) => candidate.position === change.position);
      return employer === undefined
        ? t("about.groups.position")
        : t("about.questions.role", { company: employer.company });
    }
    default: {
      return t("about.groups.goals");
    }
  }
}

export interface QuestionsStepProps {
  view: AboutView;
  /** The questions are over (the gaps ran out, or "That's enough for now"): on to the review. */
  onDone: () => void;
}

/**
 * The questions about the gaps, one at a time. Each says why it is asked: what
 * was assumed or what is not known. The question for the top gap is asked
 * for when none is on screen (one small model call); an answer is read into
 * proposed changes (one more), shown at once with where they go, and the
 * portrait gains their sentences. Skip and "That's enough for now" ask nothing
 * of the model. Nothing is saved until the review.
 */
export function QuestionsStep({ view, onDone }: QuestionsStepProps) {
  const { t, i18n } = useTranslation("profile");
  const next = useNextQuestion(i18n.language);
  const answer = useAnswerQuestion(i18n.language);
  const skip = useSkipQuestion();
  const enough = useEnoughQuestions();
  const questions = view.questions;
  const current = questions.current ?? undefined;
  const asked = questions.answered + questions.skipped;
  const waiting = current === undefined && !questions.done;
  /** How many were asked when the next one was last requested: once for each. */
  const requested = useRef(-1);
  const { mutate, reset } = next;

  useEffect(() => {
    if (waiting && requested.current !== asked) {
      requested.current = asked;
      mutate();
    }
  }, [waiting, asked, mutate]);
  useEffect(() => {
    if (questions.done) {
      onDone();
    }
  }, [questions.done, onDone]);

  const updated = view.changes.filter((change) => questions.updated.includes(change.id));
  const busy = answer.isPending || skip.isPending || enough.isPending;
  const failure = firstError(next.error, answer.error, skip.error, enough.error);

  return (
    <Stack gap="lg" testId="about-questions-step">
      {updated.length === 0 ? null : (
        <FactBlock
          title={t("about.questions.updated")}
          testId="about-q-updated"
          footer={<Text size="sm">{t("about.questions.savedLater")}</Text>}
        >
          {updated.map((change) => (
            <FactRow
              key={change.id}
              text={`${t(`about.fields.${change.field}`)}: ${change.value}`}
              quoteLabel={t("about.yourWords")}
              quote={change.quote}
              testId={`about-q-updated-${change.id}`}
            >
              <Text tone="secondary" size="sm" as="p">
                {t("about.questions.goesTo", { where: whereOf(t, change, view) })}
              </Text>
              {change.flags.length > 0 || (change.resolution ?? null) !== null ? (
                <ChangeActions
                  id={change.id}
                  value={change.value}
                  flags={change.flags}
                  resolution={change.resolution}
                />
              ) : null}
            </FactRow>
          ))}
        </FactBlock>
      )}
      {current === undefined ? (
        <Waiting
          failure={failure}
          busy={next.isPending || questions.done}
          onRetry={() => {
            requested.current = -1;
            reset();
            mutate();
          }}
          onDone={onDone}
        />
      ) : (
        <QuestionCard
          key={current.id}
          question={current}
          answered={asked}
          total={questions.total}
          strength={questions.strength}
          busy={busy}
          working={answer.isPending}
          failure={failure}
          onAnswer={(text, choices) => {
            answer.mutate({ id: current.id, text, choices });
          }}
          onSkip={() => {
            skip.mutate(current.id);
          }}
          onEnough={() => {
            enough.mutate(undefined, { onSuccess: onDone });
          }}
        />
      )}
      <Tagged testId="about-q-peek">
        <Disclosure summary={t("about.questions.peek")}>
          <Stack gap="sm">
            <AboutLegend />
            <PortraitText sentences={view.sentences} testId="about-q-portrait" />
          </Stack>
        </Disclosure>
      </Tagged>
    </Stack>
  );
}

interface WaitingProps {
  failure: unknown;
  busy: boolean;
  onRetry: () => void;
  onDone: () => void;
}

/** Between questions: the next one is being worded, or it could not be. */
function Waiting({ failure, busy, onRetry, onDone }: WaitingProps) {
  const { t } = useTranslation("profile");
  if (failure === undefined) {
    return (
      <Tagged testId="about-q-loading">
        <Text tone="secondary">{busy ? t("about.questions.asking") : t("about.loading")}</Text>
      </Tagged>
    );
  }
  return (
    <Tagged testId="about-q-error">
      <Banner
        tone="danger"
        icon="warning"
        role="alert"
        title={t(`about.errors.${errorCode(failure)}`)}
        action={
          <Cluster>
            <Button variant="secondary" data-testid="about-q-retry" onClick={onRetry}>
              {t("retry")}
            </Button>
            <Button variant="ghost" data-testid="about-q-to-review" onClick={onDone}>
              {t("about.questions.toReview")}
            </Button>
          </Cluster>
        }
      />
    </Tagged>
  );
}

interface QuestionCardProps {
  question: AboutQuestion;
  /** Questions answered or skipped so far. */
  answered: number;
  /** About how many questions in all. */
  total: number;
  /** The profile's strength now, as if saved. */
  strength: number;
  busy: boolean;
  /** The answer is being read. */
  working: boolean;
  failure: unknown;
  onAnswer: (text: string, choices: readonly string[]) => void;
  onSkip: () => void;
  onEnough: () => void;
}

/** One question: what it asks, why, the choices if it is closed, and the answer. */
function QuestionCard({
  question,
  answered,
  total,
  strength,
  busy,
  working,
  failure,
  onAnswer,
  onSkip,
  onEnough,
}: QuestionCardProps) {
  const { t } = useTranslation("profile");
  const [text, setText] = useState("");
  const [picked, setPicked] = useState<readonly string[]>([]);
  const many = MANY.has(question.kind);
  const raises = question.strength_after > strength;

  const pick = (choice: string) => {
    setPicked((current) => {
      if (current.includes(choice)) {
        return current.filter((candidate) => candidate !== choice);
      }
      return many ? [...current, choice] : [choice];
    });
  };

  return (
    <Stack gap="md" testId="about-q">
      <Stack gap="sm">
        <Text tone="secondary" size="sm" as="p">
          <span data-testid="about-q-progress">
            {t("about.questions.progress", {
              n: answered + 1,
              total: Math.max(total, answered + 1),
            })}
          </span>
          {" · "}
          <span data-testid="about-q-strength">
            {raises
              ? t("about.questions.strengthAfter", {
                  score: strength,
                  after: question.strength_after,
                })
              : t("about.questions.strength", { score: strength })}
          </span>
        </Text>
        <Heading level={2} size="title">
          <span data-testid="about-q-question">{question.question}</span>
        </Heading>
      </Stack>
      <Stack gap="sm" testId="about-q-why">
        <Cluster>
          <Tagged testId="about-q-tag">
            <Tag icon={question.tag === "unknown" ? "search" : "check"}>
              {t(`about.questions.tag.${question.tag}`)}
            </Tag>
          </Tagged>
        </Cluster>
        <Text tone="secondary" as="p">
          <span data-testid="about-q-why-text">{question.why}</span>
        </Text>
      </Stack>
      {question.choices.length === 0 ? null : (
        <Cluster>
          {question.choices.map((choice, index) => (
            <Chip
              key={choice}
              pressed={picked.includes(choice)}
              disabled={busy}
              data-testid={`about-q-choice-${String(index)}`}
              onClick={() => {
                pick(choice);
              }}
            >
              {choice}
            </Chip>
          ))}
        </Cluster>
      )}
      <TextArea
        label={t("about.questions.answerLabel")}
        value={text}
        rows={4}
        maxLength={MAX_EXTRA}
        placeholder={t("about.questions.answerPlaceholder")}
        disabled={busy}
        data-testid="about-q-answer"
        onChange={(event) => {
          setText(event.target.value);
        }}
      />
      {failure === undefined ? null : (
        <Tagged testId="about-q-error">
          <Banner
            tone="danger"
            icon="warning"
            role="alert"
            title={t(`about.errors.${errorCode(failure)}`)}
          />
        </Tagged>
      )}
      {working ? (
        <Tagged testId="about-q-working">
          <Text tone="secondary">{t("about.questions.reading")}</Text>
        </Tagged>
      ) : null}
      <Cluster>
        <Button
          variant="primary"
          icon="arrow-right"
          disabled={busy || (text.trim() === "" && picked.length === 0)}
          data-testid="about-q-send"
          onClick={() => {
            onAnswer(text, picked);
          }}
        >
          {t("about.questions.answer")}
        </Button>
        <Button variant="ghost" disabled={busy} data-testid="about-q-skip" onClick={onSkip}>
          {t("about.questions.skip")}
        </Button>
        <Button variant="ghost" disabled={busy} data-testid="about-q-enough" onClick={onEnough}>
          {t("about.questions.enough")}
        </Button>
        <Text tone="tertiary" size="sm" as="span">
          <span data-testid="about-q-cost">{t("about.questions.cost")}</span>
        </Text>
      </Cluster>
    </Stack>
  );
}
