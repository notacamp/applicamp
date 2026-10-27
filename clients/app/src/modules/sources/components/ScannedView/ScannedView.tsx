import type { TFunction } from "i18next";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Heading,
  Placeholder,
  Select,
  Stack,
  StatusText,
  Tagged,
  Text,
} from "~/lib/ui";

import { useBoards, useScans, type ScanLine, type ScanRun } from "../../api/boards";
import { formatWhen } from "../../api/format";
import { formatEuros } from "../../api/searches";

/** A run's heading: "Today 21:00 · nightly search · €0.012", "27 Oct 04:00 · discovery". */
export function runTitle(run: ScanRun, locale: string, t: TFunction<"sources">): string {
  const when = formatWhen(run.at, locale);
  const at = when.today ? t("scanned.today", { time: when.text }) : when.text;
  if (run.kind === "discovery") {
    return t("scanned.discovery", { at });
  }
  const kind = run.trigger === "nightly" ? t("scanned.nightlySearch") : t("scanned.manualSearch");
  return t("scanned.search", { at, kind, cost: formatEuros(run.cost_eur, locale) });
}

/**
 * One board's line: "Himalayas: 412 read → 30 passed your requirements → 30
 * scored by AI → 9 matched · AI €0.004", a skip, an error, or a check.
 */
export function lineText(line: ScanLine, locale: string, t: TFunction<"sources">): string {
  const name = line.board_name;
  switch (line.outcome) {
    case "checked": {
      return t("scanned.line.checked", { name, note: line.note ?? "" });
    }
    case "error": {
      return t("scanned.line.error", { name, error: line.error_kind ?? "" });
    }
    case "skipped_poll_limit": {
      return t("scanned.line.skipped", {
        name,
        read: String(line.jobs_read),
        passed: String(line.passed_requirements),
        scored: String(line.scored_by_ai),
        matched: String(line.matched),
      });
    }
    case "read": {
      return t("scanned.line.read", {
        name,
        read: String(line.jobs_read),
        passed: String(line.passed_requirements),
        scored: String(line.scored_by_ai),
        matched: String(line.matched),
        cost: formatEuros(line.ai_cost_eur, locale),
      });
    }
  }
}

/**
 * Scanned: every search and discovery run, newest first, each with what it
 * scanned on every board (jobs read, passed the person's requirements,
 * scored by AI, matched, the AI's cost, or why a board was skipped or
 * failed); filtered by board; older runs on demand.
 */
export function ScannedView() {
  const { t, i18n } = useTranslation("sources");
  const [board, setBoard] = useState<string>("all");
  const boards = useBoards();
  const scans = useScans(board === "all" ? null : board);
  const runs = scans.data?.pages.flatMap((page) => page.runs) ?? [];
  const options = [
    { value: "all", label: t("scanned.allBoards") },
    ...(boards.data?.boards ?? [])
      .filter((b) => b.status !== "declined")
      .map((b) => ({ value: b.id, label: b.name })),
  ];
  let body: ReactNode;
  if (scans.isPending) {
    body = (
      <Tagged testId="scanned-loading">
        <Text tone="secondary">{t("scanned.loading")}</Text>
      </Tagged>
    );
  } else if (scans.isError) {
    body = (
      <Tagged testId="scanned-load-error">
        <Banner tone="danger" icon="warning" title={t("scanned.loadFailed")} />
      </Tagged>
    );
  } else if (runs.length === 0) {
    body = (
      <Placeholder testId="scanned-empty" icon="search" title={t("scanned.emptyTitle")}>
        {t("scanned.emptyBody")}
      </Placeholder>
    );
  } else {
    body = (
      <Stack gap="lg">
        {runs.map((run) => (
          <Stack key={run.id} gap="sm" testId="scanned-run">
            <Heading level={2} size="title">
              {runTitle(run, i18n.language, t)}
            </Heading>
            {run.lines.map((line) => (
              <Tagged key={line.board_id} testId="scanned-line">
                {line.outcome === "error" ? (
                  <StatusText tone="problem">{lineText(line, i18n.language, t)}</StatusText>
                ) : (
                  <Text tone="secondary">{lineText(line, i18n.language, t)}</Text>
                )}
              </Tagged>
            ))}
          </Stack>
        ))}
        {scans.hasNextPage ? (
          <Button
            variant="secondary"
            disabled={scans.isFetchingNextPage}
            data-testid="scanned-more"
            onClick={() => {
              void scans.fetchNextPage();
            }}
          >
            {t("scanned.more")}
          </Button>
        ) : null}
      </Stack>
    );
  }
  return (
    <Stack>
      <Text tone="secondary">{t("scanned.lead")}</Text>
      <Select
        label={t("scanned.filter")}
        options={options}
        value={board}
        onChange={setBoard}
        data-testid="scanned-board-filter"
      />
      {body}
    </Stack>
  );
}
