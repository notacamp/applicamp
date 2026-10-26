import { useTranslation } from "react-i18next";

import { StatusText, Tagged, Text } from "~/lib/ui";

import { formatWhen, withinADay } from "../../api/format";
import { formatEuros, isRunning, type Search } from "../../api/searches";

/** What a search line needs: a whole search, or Today's summary of one. */
export type SearchSummary = Pick<
  Search,
  "status" | "trigger" | "started_at" | "finished_at" | "fetched" | "added" | "stopped"
> & { cost_eur?: number | undefined };

export interface SearchLineProps {
  search: SearchSummary;
  /** For tests: `data-testid` of the line. */
  testId?: string | undefined;
}

/**
 * One line about a search: "Last night: 412 jobs read, 9 matched", what it
 * cost, or why it stopped.
 */
export function SearchLine({ search, testId = "search-line" }: SearchLineProps) {
  const { t, i18n } = useTranslation("sources");
  const language = i18n.language;
  if (isRunning(search)) {
    return (
      <Tagged testId={testId}>
        <StatusText tone="waiting">{t("line.running")}</StatusText>
      </Tagged>
    );
  }
  if (search.status === "failed") {
    return (
      <Tagged testId={testId}>
        <StatusText tone="problem">{t("line.failed")}</StatusText>
      </Tagged>
    );
  }
  const at = search.finished_at ?? search.started_at;
  const when = formatWhen(at, language);
  const counts = t("line.counts", { fetched: String(search.fetched), added: String(search.added) });
  let lead: string;
  if (search.trigger === "nightly" && withinADay(at)) {
    lead = t("line.lastNight", { counts });
  } else if (when.today) {
    lead = t("line.today", { time: when.text, counts });
  } else {
    lead = t("line.on", { date: when.text, counts });
  }
  const cost =
    search.cost_eur === undefined
      ? null
      : t("line.cost", { cost: formatEuros(search.cost_eur, language) });
  const stopped = search.stopped ?? null;
  return (
    <Tagged testId={testId}>
      <Text tone="secondary">{[lead, cost].filter((part) => part !== null).join(" · ")}</Text>
      {stopped === null ? null : (
        <Tagged testId="search-stopped">
          <StatusText tone="problem">{t(`stopped.${stopped}`)}</StatusText>
        </Tagged>
      )}
    </Tagged>
  );
}
