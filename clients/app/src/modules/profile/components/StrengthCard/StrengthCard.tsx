import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import {
  Button,
  Icon,
  List,
  ListRow,
  Progress,
  Section,
  Stack,
  Tagged,
  Text,
  type IconName,
} from "~/lib/ui";

import {
  parseAction,
  useQuestions,
  useStrength,
  type Action,
  type StrengthItem,
} from "../../api/completion";

export interface StrengthCardProps {
  /** The person chose what to do about a part, or to improve the profile. */
  onAction: (action: Action) => void;
}

const ICONS: Record<StrengthItem["state"], IconName> = {
  ok: "check",
  partial: "clock",
  missing: "warning",
};

/** The one line under a part: what there is, or what to add. */
function hintOf(t: TFunction<"profile">, item: StrengthItem, suggested: number): string {
  const hint = t(`strength.hint.${item.key}.${item.state}`, { have: item.have, of: item.of });
  return item.key === "skills" && item.state !== "ok" && suggested > 0
    ? `${hint} · ${t("strength.skillsFound", { count: suggested })}`
    : hint;
}

/**
 * How strong the profile is: the score and a bar, what each part is worth and
 * what to do about it, and the way into "Improve my profile". Reading the
 * profile for questions starts when this shows (once per profile version).
 */
export function StrengthCard({ onAction }: StrengthCardProps) {
  const { t } = useTranslation("profile");
  const strength = useStrength();
  const questions = useQuestions();
  if (strength.isPending) {
    return (
      <Tagged testId="strength-loading">
        <Text tone="secondary">{t("strength.loading")}</Text>
      </Tagged>
    );
  }
  if (strength.isError) {
    return null;
  }
  const { score, strong, items } = strength.data;
  const suggested = strength.data.suggested_skills ?? 0;
  const open = questions.data?.status === "ready" ? questions.data.questions.length : 0;
  const reading = questions.data?.status === "running";

  const improve = (
    <Button
      variant="primary"
      icon="arrow-right"
      data-testid="strength-improve"
      onClick={() => {
        onAction({ kind: "improve" });
      }}
    >
      {open > 0 ? t("strength.improveCount", { count: open }) : t("strength.improve")}
    </Button>
  );

  return (
    <Section
      title={t("strength.title")}
      description={t("strength.description")}
      action={strong && open === 0 ? undefined : improve}
      testId="strength-card"
      anchor="strength"
    >
      <Stack gap="md">
        <Tagged testId="strength-score">
          <Stack gap="sm">
            <Text size="lg" weight="semibold" numeric>
              {t("strength.score", { score })}
            </Text>
            <Progress label={t("strength.scoreLabel", { score })} value={score} max={100}>
              {strong ? t("strength.strong") : t("strength.growing", { left: 80 - score })}
            </Progress>
          </Stack>
        </Tagged>
        {reading ? (
          <Tagged testId="strength-reading">
            <Text tone="secondary" size="sm">
              {t("strength.reading")}
            </Text>
          </Tagged>
        ) : null}
        <List label={t("strength.checklist")}>
          {items.map((item) => {
            const action = parseAction(item.action);
            return (
              <ListRow
                key={item.key}
                leading={<Icon name={ICONS[item.state]} />}
                title={t(`strength.parts.${item.key}`)}
                meta={hintOf(t, item, suggested)}
                trailing={
                  action === null ? null : (
                    <Button
                      variant="secondary"
                      data-testid={`strength-${item.key}-action`}
                      onClick={() => {
                        onAction(action);
                      }}
                    >
                      {t(item.state === "partial" ? "strength.improveItem" : "strength.add")}
                    </Button>
                  )
                }
              />
            );
          })}
        </List>
        <Text tone="tertiary" size="sm">
          {t("strength.note")}
        </Text>
      </Stack>
    </Section>
  );
}
