import { useNavigate } from "@tanstack/react-router";
import type { MouseEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { formatDay } from "~/lib/i18n";
import { Banner, Button, ButtonLink, Page, Placeholder, Stack, Tagged, Text } from "~/lib/ui";
import { useToday, type Today as TodayState } from "~/modules/pipeline";
import { SearchLine, SearchNowButton } from "~/modules/sources";

import { useImportLink } from "../links";
import { NoSkillsLine } from "../NoSkillsLine";

/** An in-app link: a real `href` (a new tab with ⌘), else `go` without a reload. */
function inAppLink(href: string, go: () => void) {
  return {
    href,
    onClick: (event: MouseEvent<HTMLElement>) => {
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) {
        return;
      }
      event.preventDefault();
      go();
    },
  };
}

/**
 * `/`: what waits on you, from where you are: set up the profile, answer the
 * goals, run a first search, or review the new matches; with the latest
 * search's line. "Nothing needs you yet" only when that is true.
 */
export function Today() {
  const { t, i18n } = useTranslation();
  const today = useToday();
  let body: ReactNode;
  if (today.isPending) {
    body = (
      <Tagged testId="today-loading">
        <Text tone="secondary">{t("today.loading")}</Text>
      </Tagged>
    );
  } else if (today.isError) {
    body = (
      <Tagged testId="today-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("today.loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void today.refetch();
              }}
            >
              {t("today.retry")}
            </Button>
          }
        />
      </Tagged>
    );
  } else {
    body = <TodayBody state={today.data} />;
  }
  return (
    <Page
      testId="today"
      eyebrow={formatDay(i18n.language)}
      title={t("today.title")}
      lead={t("today.lead")}
    >
      {body}
    </Page>
  );
}

function TodayBody({ state }: { state: TodayState }) {
  const { t } = useTranslation();
  const importLink = useImportLink();
  const navigate = useNavigate();
  const latest = state.latest_search ?? null;
  const hint = state.profile.strength_hint ?? null;

  let step: ReactNode = null;
  switch (state.next_step) {
    case "set_up_profile": {
      step = (
        <Placeholder
          testId="today-setup"
          icon="people"
          title={t("today.setUpTitle")}
          action={
            <ButtonLink
              variant="primary"
              icon="arrow-right"
              data-testid="set-up-profile"
              {...importLink}
            >
              {t("today.setUp")}
            </ButtonLink>
          }
        >
          {t("today.setUpBody")}
        </Placeholder>
      );
      break;
    }
    case "answer_goals": {
      const goals = inAppLink("/goals/interview", () => {
        void navigate({ to: "/goals/interview" });
      });
      step = (
        <Placeholder
          testId="today-goals"
          icon="check"
          title={t("today.goalsTitle")}
          action={
            <ButtonLink
              variant="primary"
              icon="arrow-right"
              data-testid="today-answer-goals"
              {...goals}
            >
              {t("today.answerGoals")}
            </ButtonLink>
          }
        >
          {t("today.goalsBody", {
            answered: String(state.goals.answered),
            total: String(state.goals.total),
          })}
        </Placeholder>
      );
      break;
    }
    case "search": {
      step = (
        <Placeholder
          testId="today-search"
          icon="search"
          title={t("today.searchTitle")}
          action={<SearchNowButton testId="today-search-now" />}
        >
          {t("today.searchBody")}
        </Placeholder>
      );
      break;
    }
    case "review_matches": {
      const pipeline = inAppLink("/pipeline?new=true", () => {
        void navigate({ to: "/pipeline", search: { new: true } });
      });
      step = (
        <Placeholder
          testId="today-matches"
          icon="tasks"
          title={t("today.matchesTitle", { count: String(state.new_matches) })}
          action={
            <ButtonLink
              variant="primary"
              icon="arrow-right"
              data-testid="today-review-matches"
              {...pipeline}
            >
              {t("today.reviewMatches")}
            </ButtonLink>
          }
        >
          {t("today.matchesBody")}
        </Placeholder>
      );
      break;
    }
    case "none": {
      step = (
        <Placeholder
          testId="today-nothing"
          icon="check"
          title={t("today.nothingTitle")}
          action={<SearchNowButton testId="today-search-again" variant="secondary" />}
        >
          {t("today.nothingBody")}
        </Placeholder>
      );
      break;
    }
  }

  return (
    <Stack>
      {step}
      {latest === null ? null : <SearchLine search={latest} testId="today-latest-search" />}
      {state.next_step === "set_up_profile" ? null : <NoSkillsLine testId="today-no-skills" />}
      {hint === null ||
      state.next_step === "set_up_profile" ||
      (state.profile.no_skills && hint === "add_skills") ? null : (
        <Tagged testId="today-strength-hint">
          <Text tone="secondary">{t(`today.strength.${hint}`)}</Text>
        </Tagged>
      )}
    </Stack>
  );
}
