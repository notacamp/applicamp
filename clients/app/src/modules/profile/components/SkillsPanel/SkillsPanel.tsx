import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Cluster,
  IconButton,
  List,
  ListRow,
  Progress,
  Section,
  Stack,
  Tag,
  Tagged,
  Text,
} from "~/lib/ui";

import {
  useAddSkill,
  useDismissSkill,
  useOrderSkills,
  useReviewAgain,
  useSuggestedSkills,
} from "../../api/completion";
import { useProfile } from "../../api/profile";

/** How many skills drive the job board searches. */
const TOP = 3;

const REASONS = [
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_budget_reached",
  "ai_failed",
] as const;

/** Why the skills could not be found, if the messages know it, else `ai_failed`. */
function reasonCode(reason: string | null | undefined): (typeof REASONS)[number] {
  return REASONS.find((known) => known === reason) ?? "ai_failed";
}

export interface SkillsPanelProps {
  /** Back to Profile & goals. */
  onBack: () => void;
}

/** `list` with the item at `from` moved to `to`. */
function moved<T>(list: readonly T[], from: number, to: number): T[] {
  const next = [...list];
  const [item] = next.splice(from, 1);
  if (item !== undefined) {
    next.splice(to, 0, item);
  }
  return next;
}

/**
 * Skills: the profile's own, in the order they count (the first three drive
 * the job board searches; move them up or down), and the skills found in the
 * person's positions that the list lacks, each Add or Not mine. Nothing is
 * added without a click.
 */
export function SkillsPanel({ onBack }: SkillsPanelProps) {
  const { t } = useTranslation("profile");
  const profile = useProfile();
  const suggested = useSuggestedSkills();
  const add = useAddSkill();
  const dismiss = useDismissSkill();
  const order = useOrderSkills();
  const again = useReviewAgain();

  const skills = profile.data?.profile.skills ?? [];
  const names = skills.map((skill) => skill.name);
  const move = (from: number, to: number) => {
    order.mutate(moved(names, from, to));
  };

  let found;
  if (suggested.isPending || suggested.data?.status === "running") {
    found = (
      <Tagged testId="skills-reading">
        <Progress label={t("skills.reading")} value={0} max={0}>
          {t("skills.reading")}
        </Progress>
      </Tagged>
    );
  } else if (suggested.isError) {
    found = (
      <Tagged testId="skills-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("skills.loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void suggested.refetch();
              }}
            >
              {t("retry")}
            </Button>
          }
        />
      </Tagged>
    );
  } else if (suggested.data.status === "unavailable" || suggested.data.status === "failed") {
    const failed = suggested.data.status === "failed";
    found = (
      <Tagged testId={failed ? "skills-failed" : "skills-unavailable"}>
        <Banner
          icon="warning"
          title={t(
            failed ? `skills.reasons.${reasonCode(suggested.data.reason)}` : "skills.unavailable",
          )}
          action={
            failed ? (
              <Button
                variant="secondary"
                disabled={again.isPending}
                data-testid="skills-review-again"
                onClick={() => {
                  again.mutate();
                }}
              >
                {t("retry")}
              </Button>
            ) : undefined
          }
        />
      </Tagged>
    );
  } else if (suggested.data.skills.length === 0) {
    found = (
      <Tagged testId="skills-none-found">
        <Text tone="secondary">{t("skills.noneFound")}</Text>
      </Tagged>
    );
  } else {
    found = (
      <List label={t("skills.found")}>
        {suggested.data.skills.map((skill) => (
          <ListRow
            key={skill.name}
            title={skill.name}
            meta={
              <Tagged testId={`skill-roles-${skill.name}`}>
                {t("skills.inRoles", { count: skill.roles })}
              </Tagged>
            }
            trailing={
              <Cluster>
                <Button
                  variant="secondary"
                  disabled={add.isPending || dismiss.isPending}
                  aria-label={t("skills.addLabel", { name: skill.name })}
                  data-testid={`skill-add-${skill.name}`}
                  onClick={() => {
                    add.mutate(skill.name);
                  }}
                >
                  {t("skills.add")}
                </Button>
                <Button
                  variant="ghost"
                  disabled={add.isPending || dismiss.isPending}
                  aria-label={t("skills.notMineLabel", { name: skill.name })}
                  data-testid={`skill-dismiss-${skill.name}`}
                  onClick={() => {
                    dismiss.mutate(skill.name);
                  }}
                >
                  {t("skills.notMine")}
                </Button>
              </Cluster>
            }
          />
        ))}
      </List>
    );
  }

  return (
    <Stack gap="lg" testId="skills-panel">
      <Section
        title={t("skills.yours")}
        description={t("skills.order")}
        testId="skills-yours"
        action={
          <Button variant="ghost" icon="chevron-left" data-testid="skills-back" onClick={onBack}>
            {t("skills.back")}
          </Button>
        }
      >
        {profile.isPending ? (
          <Text tone="secondary">{t("loading")}</Text>
        ) : skills.length === 0 ? (
          <Tagged testId="skills-empty">
            <Text tone="secondary">{t("skills.empty")}</Text>
          </Tagged>
        ) : (
          <List label={t("skills.yours")}>
            {skills.map((skill, index) => (
              <ListRow
                key={skill.name}
                leading={
                  <Tagged testId={`skill-rank-${skill.name}`}>
                    <Text numeric tone="secondary">
                      {index + 1}
                    </Text>
                  </Tagged>
                }
                title={skill.name}
                badge={
                  index < TOP ? (
                    <Tagged testId={`skill-top-${skill.name}`}>
                      <Tag icon="search">{t("skills.top")}</Tag>
                    </Tagged>
                  ) : undefined
                }
                trailing={
                  <Cluster>
                    <IconButton
                      label={t("skills.moveUp", { name: skill.name })}
                      icon="chevron-up"
                      disabled={index === 0 || order.isPending}
                      data-testid={`skill-up-${skill.name}`}
                      onClick={() => {
                        move(index, index - 1);
                      }}
                    />
                    <IconButton
                      label={t("skills.moveDown", { name: skill.name })}
                      icon="chevron-down"
                      disabled={index === skills.length - 1 || order.isPending}
                      data-testid={`skill-down-${skill.name}`}
                      onClick={() => {
                        move(index, index + 1);
                      }}
                    />
                  </Cluster>
                }
              />
            ))}
          </List>
        )}
        {order.isError ? (
          <Tagged testId="skills-order-error">
            <Banner tone="danger" icon="warning" role="alert" title={t("form.saveFailed")} />
          </Tagged>
        ) : null}
      </Section>
      <Section
        title={t("skills.foundTitle")}
        description={t("skills.foundLead")}
        testId="skills-found"
      >
        <Stack gap="sm">
          {found}
          {add.isError || dismiss.isError ? (
            <Tagged testId="skills-action-error">
              <Banner tone="danger" icon="warning" role="alert" title={t("form.saveFailed")} />
            </Tagged>
          ) : null}
          <Text tone="tertiary" size="sm">
            {t("skills.foundNote")}
          </Text>
        </Stack>
      </Section>
    </Stack>
  );
}
