import { useEffect, useEffectEvent, useRef } from "react";
import { useTranslation } from "react-i18next";

import { Progress, Tagged } from "~/lib/ui";

import { isRunning, useLatestSearch } from "../../api/searches";

import { SearchLine } from "./SearchLine";

export interface SearchProgressProps {
  /** A search seen running has just ended (refetch what it added). */
  onFinished?: (() => void) | undefined;
}

/**
 * The latest search: its progress while it runs (jobs read, scored, added),
 * then its one-line summary. Nothing before the first search.
 */
export function SearchProgress({ onFinished }: SearchProgressProps) {
  const { t } = useTranslation("sources");
  const latest = useLatestSearch();
  const search = latest.data ?? null;
  const running = isRunning(search);
  const wasRunning = useRef(false);
  const finished = useEffectEvent(() => {
    onFinished?.();
  });
  useEffect(() => {
    if (wasRunning.current && !running) {
      finished();
    }
    wasRunning.current = running;
  }, [running]);

  if (search === null) {
    return null;
  }
  if (running) {
    return (
      <Tagged testId="search-progress">
        <Progress label={t("progress.label")} value={search.scored} max={search.kept}>
          {search.status === "queued"
            ? t("progress.queued")
            : t("progress.running", {
                fetched: String(search.fetched),
                scored: String(search.scored),
                added: String(search.added),
              })}
        </Progress>
      </Tagged>
    );
  }
  return <SearchLine search={search} />;
}
