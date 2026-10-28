import { useNavigate } from "@tanstack/react-router";
import type { MouseEvent } from "react";
import { useTranslation } from "react-i18next";

import { ButtonLink, Cluster, Tagged, Text } from "~/lib/ui";
import { useToday } from "~/modules/pipeline";

export interface NoSkillsLineProps {
  /** For tests: `data-testid` of the line. */
  testId?: string | undefined;
}

/**
 * "Your profile lists no skills yet. Add them for better matches", linking
 * to Skills on Profile & goals. Shown only while the profile lists no
 * skills and names no technology: until then matches cannot cite a stack.
 */
export function NoSkillsLine({ testId = "no-skills-line" }: NoSkillsLineProps) {
  const { t } = useTranslation();
  const today = useToday();
  const navigate = useNavigate();
  if (today.data?.profile.no_skills !== true || !today.data.profile.done) {
    return null;
  }
  const go = (event: MouseEvent<HTMLElement>) => {
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) {
      return;
    }
    event.preventDefault();
    void navigate({ to: "/profile", hash: "skills" });
  };
  return (
    <Tagged testId={testId}>
      <Cluster>
        <Text tone="secondary">{t("noSkills.line")}</Text>
        <ButtonLink
          variant="secondary"
          href="/profile#skills"
          onClick={go}
          data-testid="no-skills-link"
        >
          {t("noSkills.action")}
        </ButtonLink>
      </Cluster>
    </Tagged>
  );
}
