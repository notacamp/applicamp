import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Stack, Tagged, Text } from "~/lib/ui";

import {
  preparationReason,
  useStartPreparation,
  useStopPreparation,
  type Brief,
  type Preparation,
} from "../../api/brief";
import { formatCost, formatShortDate } from "../../api/labels";

export interface PreparationPanelProps {
  id: string;
  brief: Brief;
  /** Open the review of the CV (and the letter beside it). */
  onReview: () => void;
}

/**
 * What the app does by itself for this job (ADR 0010): what it is doing or
 * waiting on, the steps it did with what they cost, and the person's controls:
 * Stop, "Prepare by itself" and Try again. Approving stays the person's.
 */
export function PreparationPanel({ id, brief, onReview }: PreparationPanelProps) {
  const { t, i18n } = useTranslation("pipeline");
  const language = i18n.language;
  const start = useStartPreparation(id, language);
  const stop = useStopPreparation(id);
  const preparation = brief.preparation ?? null;
  const company = brief.company;

  if (preparation === null) {
    return brief.prepares_by_itself ? null : (
      <Tagged testId="prepare-by-click">
        <Text tone="secondary" size="sm">
          {t("prepare.byItself.off")}
        </Text>
      </Tagged>
    );
  }

  const stopButton = (
    <Button
      variant="secondary"
      disabled={stop.isPending}
      data-testid="prepare-stop"
      onClick={() => {
        stop.mutate();
      }}
    >
      {t("prepare.byItself.stop")}
    </Button>
  );
  const startButton = (label: string) =>
    brief.prepares_by_itself ? (
      <Button
        variant="secondary"
        disabled={start.isPending}
        data-testid="prepare-start"
        onClick={() => {
          start.mutate();
        }}
      >
        {label}
      </Button>
    ) : undefined;

  return (
    <Stack gap="sm">
      <Tagged testId={`prepare-state-${preparation.state}`}>
        <StateBanner
          preparation={preparation}
          company={company}
          stopButton={stopButton}
          review={
            <Button variant="primary" data-testid="prepare-by-itself-review" onClick={onReview}>
              {t("prepare.byItself.review")}
            </Button>
          }
          startButton={startButton}
        />
      </Tagged>
      {preparation.done.length === 0 ? null : (
        <Tagged testId="prepare-done">
          <Text tone="secondary" size="sm">
            {t("prepare.byItself.did", {
              steps: preparation.done
                .map((step) =>
                  t(`prepare.byItself.steps.${stepKey(step.step)}`, {
                    date: formatShortDate(step.at, language),
                  }),
                )
                .join(", "),
              cost: formatCost(preparation.spent_eur, language),
            })}
          </Text>
        </Tagged>
      )}
      {start.isError || stop.isError ? (
        <Tagged testId="prepare-control-error">
          <Text tone="secondary" size="sm">
            {t("prepare.errors.generic")}
          </Text>
        </Tagged>
      ) : null}
    </Stack>
  );
}

/** The steps the page words (`prepare.byItself.steps.<step>`). */
const STEPS = ["shortlist", "site", "read", "compare", "draft", "letter"] as const;

function stepKey(step: string): (typeof STEPS)[number] | "other" {
  return STEPS.find((known) => known === step) ?? "other";
}

function StateBanner({
  preparation,
  company,
  stopButton,
  review,
  startButton,
}: {
  preparation: Preparation;
  company: string;
  stopButton: ReactNode;
  review: ReactNode;
  startButton: (label: string) => ReactNode;
}) {
  const { t } = useTranslation("pipeline");
  switch (preparation.state) {
    case "queued":
    case "running": {
      return (
        <Banner
          role="status"
          icon="hourglass"
          title={t(`prepare.byItself.working.${preparation.step}`, { company })}
          action={stopButton}
        >
          {t("prepare.byItself.workingLead")}
        </Banner>
      );
    }
    case "waiting": {
      const on = preparation.waiting_on ?? "answers";
      const approval = on === "cv_approval" || on === "letter_approval";
      return (
        <Banner
          icon={approval ? "check" : "clock"}
          title={t(`prepare.byItself.waiting.${on}`, { company })}
          action={
            approval ? (
              <Cluster>
                {review}
                {stopButton}
              </Cluster>
            ) : (
              stopButton
            )
          }
        />
      );
    }
    case "done": {
      return <Banner icon="check" title={t("prepare.byItself.done")} />;
    }
    case "stopped": {
      const reason = preparationReason(preparation.reason);
      return (
        <Banner
          icon="clock"
          title={t(`prepare.byItself.reasons.${reason}`)}
          action={startButton(t("prepare.byItself.start"))}
        >
          {t("prepare.byItself.stoppedLead")}
        </Banner>
      );
    }
    case "failed": {
      const reason = preparationReason(preparation.reason);
      return (
        <Banner
          tone="danger"
          icon="warning"
          title={t(`prepare.byItself.reasons.${reason}`)}
          action={startButton(t("prepare.byItself.tryAgain"))}
        >
          {t("prepare.byItself.failedLead")}
        </Banner>
      );
    }
  }
}
