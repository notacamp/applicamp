import type { MouseEvent } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { readPref, writePref } from "~/lib/storage/prefs";
import { Button, ButtonLink, Cluster, Placeholder, Progress, Stack, Text } from "~/lib/ui";

/** Where "Continue" goes. */
export type NudgeAction = "improve" | "skills" | "import" | "goals";

export interface StrengthNudgeProps {
  /** The profile strength, 0 to 100. */
  score: number;
  /** Questions that would make it stronger, when the AI has read the profile. */
  questions: number | null | undefined;
  /** What to do next. */
  action: NudgeAction;
  /** Continue is a link: its address and its in-app click. */
  link: { href: string; onClick: (event: MouseEvent<HTMLElement>) => void };
  /** For tests: the time now. */
  now?: Date;
}

/** How long "Not now" puts the card away. */
const HIDDEN_DAYS = 7;

/** Whether the card was put away and the week has not ended. */
export function isPutAway(now: Date): boolean {
  const until = readPref("profile.nudge");
  if (until === null) {
    return false;
  }
  const time = Date.parse(until);
  return !Number.isNaN(time) && time > now.getTime();
}

/**
 * Today's card while the profile is not strong: its strength and one way to
 * go on. "Not now" puts it away for a week; it never blocks anything.
 */
export function StrengthNudge({ score, questions, action, link, now }: StrengthNudgeProps) {
  const { t } = useTranslation("profile");
  const [hidden, setHidden] = useState(() => isPutAway(now ?? new Date()));
  if (hidden) {
    return null;
  }
  const body =
    action === "improve" && questions !== null && questions !== undefined && questions > 0
      ? t("nudge.bodyQuestions", { count: questions })
      : t(`nudge.body.${action}`);
  return (
    <Placeholder
      testId="today-profile-nudge"
      icon="people"
      title={t("nudge.title", { score })}
      action={
        <Cluster>
          <ButtonLink variant="primary" icon="arrow-right" data-testid="nudge-continue" {...link}>
            {t("nudge.continue")}
          </ButtonLink>
          <Button
            variant="ghost"
            data-testid="nudge-not-now"
            onClick={() => {
              const until = new Date((now ?? new Date()).getTime() + HIDDEN_DAYS * 86_400_000);
              writePref("profile.nudge", until.toISOString());
              setHidden(true);
            }}
          >
            {t("nudge.notNow")}
          </Button>
        </Cluster>
      }
    >
      <Stack gap="sm">
        <Text>{body}</Text>
        <Progress label={t("strength.scoreLabel", { score })} value={score} max={100} />
      </Stack>
    </Placeholder>
  );
}
