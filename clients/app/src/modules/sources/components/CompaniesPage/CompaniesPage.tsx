import { useTranslation } from "react-i18next";

import { Banner, Button, Placeholder, Stack, Tagged, Text, ViewSwitch } from "~/lib/ui";

import { useCompanies } from "../../api/companies";
import { HowItWorks } from "./HowItWorks";
import { SuggestionCard } from "./SuggestionCard";
import { WatchingList } from "./WatchingList";

/** The Companies page's tabs. */
export type CompaniesTab = "suggested" | "watching" | "how";

export interface CompaniesPageProps {
  tab: CompaniesTab;
  onTab: (tab: CompaniesTab) => void;
}

/**
 * Companies worth watching: the ones whose jobs keep fitting the person
 * (Suggested, each with its traced reasons), the ones whose own job board is
 * read every day (Watching), and how it works and what it costs.
 */
export function CompaniesPage({ tab, onTab }: CompaniesPageProps) {
  const { t } = useTranslation("sources");
  const companies = useCompanies();
  if (companies.isPending) {
    return (
      <Tagged testId="companies-loading">
        <Text tone="secondary">{t("companies.loading")}</Text>
      </Tagged>
    );
  }
  if (companies.isError) {
    return (
      <Tagged testId="companies-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("companies.loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void companies.refetch();
              }}
              data-testid="companies-retry"
            >
              {t("companies.retry")}
            </Button>
          }
        />
      </Tagged>
    );
  }
  const list = companies.data;
  return (
    <Stack gap="lg">
      <ViewSwitch
        label={t("companies.tabs.label")}
        options={[
          {
            value: "suggested" as const,
            label: t("companies.tabs.suggestedCount", { n: String(list.suggested.length) }),
            icon: "star",
            testId: "companies-tab-suggested",
          },
          {
            value: "watching" as const,
            label: t("companies.tabs.watchingCount", { n: String(list.watching.length) }),
            icon: "stay",
            testId: "companies-tab-watching",
          },
          {
            value: "how" as const,
            label: t("companies.tabs.how"),
            icon: "preview",
            testId: "companies-tab-how",
          },
        ]}
        value={tab}
        onChange={onTab}
      />
      {tab === "suggested" ? (
        list.suggested.length === 0 ? (
          <Placeholder
            testId="companies-suggested-empty"
            icon="star"
            title={t("companies.suggested.emptyTitle")}
          >
            {t("companies.suggested.emptyBody")}
          </Placeholder>
        ) : (
          <Stack>
            {list.suggested.map((suggestion) => (
              <SuggestionCard
                key={suggestion.id}
                suggestion={suggestion}
                onWatching={() => {
                  onTab("watching");
                }}
              />
            ))}
          </Stack>
        )
      ) : null}
      {tab === "watching" ? <WatchingList list={list} /> : null}
      {tab === "how" ? <HowItWorks max={list.limit} /> : null}
    </Stack>
  );
}
