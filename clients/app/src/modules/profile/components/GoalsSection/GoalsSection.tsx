import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Cluster,
  InlineEdit,
  Property,
  PropertyList,
  Section,
  Stack,
  Tagged,
  Text,
} from "~/lib/ui";

import { useGoals, useSaveGoals, type Goals } from "../../api/goals";
import {
  answeredCount,
  firstOpen,
  pendingOf,
  QUESTIONS,
  questionsFor,
  sameAnswers,
  settle,
  type Pending,
  type Question,
} from "../../api/questions";
import { GoalQuestion } from "../GoalQuestion/GoalQuestion";
import { useGoalLabels } from "../GoalQuestion/labels";

export interface GoalsSectionProps {
  /** The profile was just imported: offer the questions, up front. */
  offer: boolean;
  /** Put the offer away. */
  onDismissOffer: () => void;
  /** Open the interview at `question`. */
  onStart: (question: Question) => void;
}

interface Editing {
  question: Question;
  working: Goals;
  pending: Pending;
  amountInvalid: boolean;
}

/**
 * The Goals section of Profile & goals: every answer as a property, edited
 * in place with the interview's own controls, and the way into the
 * questions (to start them, or to go on where they were left).
 */
export function GoalsSection({ offer, onDismissOffer, onStart }: GoalsSectionProps) {
  const { t } = useTranslation("profile");
  const goals = useGoals();
  const save = useSaveGoals();
  const labels = useGoalLabels();
  const [editing, setEditing] = useState<Editing | null>(null);

  if (goals.isPending) {
    return (
      <Tagged testId="goals-loading">
        <Text tone="secondary">{t("goals.loading")}</Text>
      </Tagged>
    );
  }
  if (goals.isError) {
    return (
      <Tagged testId="goals-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("goals.loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void goals.refetch();
              }}
            >
              {t("retry")}
            </Button>
          }
        />
      </Tagged>
    );
  }

  const { answers, version } = goals.data;
  const answered = answeredCount(answers);
  const asked = questionsFor(answers);
  const started = answered > 0;
  const summaryOf = (question: Question) => labels.summary(answers, question);

  const edit = (question: Question) => {
    setEditing({
      question,
      working: answers,
      pending: pendingOf(answers),
      amountInvalid: false,
    });
  };
  const keep = () => {
    if (editing === null) {
      return;
    }
    const result = settle(editing.question, editing.working, editing.pending);
    if (result === "invalid") {
      setEditing({ ...editing, amountInvalid: true });
      return;
    }
    if (sameAnswers(result, answers)) {
      setEditing(null);
      return;
    }
    save.mutate(result, {
      onSuccess: () => {
        setEditing(null);
      },
    });
  };

  const start = (
    <Button
      variant={started ? "secondary" : "primary"}
      icon="arrow-right"
      data-testid={started ? "goals-continue" : "goals-start"}
      onClick={() => {
        onStart(started ? firstOpen(answers) : QUESTIONS[0]);
      }}
    >
      {t(started ? "goals.section.continue" : "goals.section.start")}
    </Button>
  );

  return (
    <Section
      title={t("goals.section.title")}
      description={t("goals.section.description")}
      action={start}
      testId="goals-section"
    >
      {offer && !started ? (
        <Tagged testId="goals-offer">
          <Banner
            icon="check"
            title={t("goals.section.offerTitle")}
            action={
              <Cluster>
                <Button
                  variant="primary"
                  icon="arrow-right"
                  data-testid="goals-offer-start"
                  onClick={() => {
                    onStart(QUESTIONS[0]);
                  }}
                >
                  {t("goals.section.offerStart")}
                </Button>
                <Button variant="ghost" data-testid="goals-offer-dismiss" onClick={onDismissOffer}>
                  {t("goals.section.notNow")}
                </Button>
              </Cluster>
            }
          >
            {t("goals.section.offerBody")}
          </Banner>
        </Tagged>
      ) : null}
      <Stack gap="sm">
        <PropertyList testId="goals-properties">
          {asked.map((question) => {
            const label = t(`goals.q.${question}.label`);
            const open = editing?.question === question ? editing : null;
            return (
              <Property key={question} label={label}>
                <InlineEdit
                  label={label}
                  summary={summaryOf(question)}
                  placeholder={t(`goals.q.${question}.empty`)}
                  editing={open !== null}
                  onEdit={() => {
                    edit(question);
                  }}
                  onSave={keep}
                  onCancel={() => {
                    setEditing(null);
                  }}
                  saveLabel={t("form.save")}
                  cancelLabel={t("form.cancel")}
                  busy={save.isPending}
                  testId={`goal-${question}`}
                >
                  {open === null ? null : (
                    <GoalQuestion
                      question={question}
                      answers={open.working}
                      onAnswers={(working) => {
                        setEditing((now) => (now === null ? now : { ...now, working }));
                      }}
                      pending={open.pending}
                      onPending={(pending) => {
                        setEditing((now) =>
                          now === null ? now : { ...now, pending, amountInvalid: false },
                        );
                      }}
                      amountInvalid={open.amountInvalid}
                    />
                  )}
                </InlineEdit>
              </Property>
            );
          })}
        </PropertyList>
        <Tagged testId="goals-status">
          <Text tone="tertiary" size="sm">
            {t("goals.section.status", { answered, total: asked.length })}
            {version > 0 ? ` · ${t("goals.section.version", { version })}` : ""}
          </Text>
        </Tagged>
        {save.isError ? (
          <Tagged testId="goals-save-error">
            <Banner tone="danger" icon="warning" role="alert" title={t("form.saveFailed")} />
          </Tagged>
        ) : null}
      </Stack>
    </Section>
  );
}
