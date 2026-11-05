import type { TFunction } from "i18next";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Bullets,
  Button,
  Cluster,
  Figures,
  Heading,
  Placeholder,
  Property,
  PropertyList,
  RowItem,
  RowList,
  Stack,
  StatusText,
  Tagged,
  Text,
  TouchTargets,
  type Figure,
} from "~/lib/ui";

import { useScans, type ScanLine, type ScanRun, type SearchFunnel } from "../../api/boards";
import { formatWhen, withinADay } from "../../api/format";
import { formatEuros, isRunning, useEstimate, useLatestSearch } from "../../api/searches";
import { SearchNowButton } from "../SearchNow/SearchNowButton";

/** The reasons jobs are left out, in the order the list says them. */
export const LEFT_OUT_REASONS = [
  "seen_before",
  "too_old",
  "not_remote",
  "quick_look",
  "preselect",
  "language",
  "not_open_to_location",
  "below_floor",
  "not_good_enough",
  "waiting",
] as const;

/** A count in `locale`: "1,112". */
function formatCount(value: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(value);
}

/** What some AI cost: "€0.09", or "under €0.001" for less than a tenth of a cent. */
export function costText(value: number, locale: string, t: TFunction<"sources">): string {
  return value > 0 && value < 0.001
    ? t("scanned.costUnder", { cost: formatEuros(0.001, locale) })
    : formatEuros(value, locale);
}

/** A day in `locale`: "3 Nov". */
function formatDate(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale === "en" ? "en-GB" : locale, {
    day: "numeric",
    month: "short",
  }).format(new Date(iso));
}

/**
 * A run's name: "Last night · 3 Nov" for the nightly search of the last day,
 * "2 Nov" for an older one, "You searched · Today 10:30" for Search now,
 * "Looked for new job sites · 27 Oct" for discovery.
 */
export function runTitle(
  run: ScanRun,
  locale: string,
  t: TFunction<"sources">,
  now: Date = new Date(),
): string {
  if (run.kind === "discovery") {
    return t("scanned.discovery", { date: formatDate(run.at, locale) });
  }
  if (run.trigger === "nightly") {
    const date = formatDate(run.at, locale);
    return withinADay(run.at, now) ? t("scanned.lastNight", { date }) : date;
  }
  const when = formatWhen(run.at, locale, now);
  const at = when.today ? t("scanned.today", { time: when.text }) : when.text;
  return t("scanned.manual", { at });
}

/** A search's four numbers: jobs read, read in full, good matches, cost. */
export function runFigures(
  run: ScanRun,
  funnel: SearchFunnel,
  locale: string,
  t: TFunction<"sources">,
): Figure[] {
  return [
    {
      value: formatCount(funnel.read, locale),
      label: t("scanned.figures.read", { count: funnel.read }),
      testId: "scanned-read",
    },
    {
      value: formatCount(funnel.read_in_full, locale),
      label: t("scanned.figures.readInFull"),
      testId: "scanned-read-in-full",
    },
    {
      value: formatCount(funnel.good_matches, locale),
      label: t("scanned.figures.goodMatches", { count: funnel.good_matches }),
      testId: "scanned-good-matches",
    },
    {
      value: costText(run.cost_eur, locale, t),
      label: t("scanned.figures.cost"),
      testId: "scanned-cost",
    },
  ];
}

/** Why the jobs were left out, the reasons with a count, each "Seen before · 431". */
export function leftOutLines(
  funnel: SearchFunnel,
  locale: string,
  t: TFunction<"sources">,
): string[] {
  const left = funnel.left_out;
  if (left === null || left === undefined) {
    return [];
  }
  return LEFT_OUT_REASONS.flatMap((reason) => {
    const count = left[reason] ?? 0;
    return count > 0
      ? [
          t("scanned.count", {
            label: t(`scanned.reasons.${reason}`),
            n: formatCount(count, locale),
          }),
        ]
      : [];
  });
}

/**
 * Where the matches came from, by site, the companies' own job pages
 * together: "Himalayas · 2", "Company sites · 1".
 */
export function matchLines(lines: readonly ScanLine[], locale: string, t: TFunction<"sources">) {
  const bySite = new Map<string, number>();
  for (const line of lines) {
    if (line.matched > 0) {
      const name = line.board_slug === "watch" ? t("scanned.companySites") : line.board_name;
      bySite.set(name, (bySite.get(name) ?? 0) + line.matched);
    }
  }
  return [...bySite.entries()]
    .toSorted(([, a], [, b]) => b - a)
    .map(([label, count]) => t("scanned.count", { label, n: formatCount(count, locale) }));
}

/** The words a site's failure is told in. */
function errorText(kind: string | null | undefined, t: TFunction<"sources">): string {
  switch (kind) {
    case "rate_limited":
    case "backing_off": {
      return t("scanned.errors.slowDown");
    }
    case "needs_key": {
      return t("scanned.errors.needsKey");
    }
    case "bad_config": {
      return t("scanned.errors.badConfig");
    }
    default: {
      return t("scanned.errors.noAnswer");
    }
  }
}

/** A page that failed after the first, as the server notes it ("Read 2 pages. Page 3 did not answer."). */
const PARTIAL = /^Read (\d+) pages?\. Page (\d+) did not answer\.$/;

/**
 * What a search did on one site: "412 jobs read · 30 given a quick look · 12
 * read in full · 4 good matches · €0.004", a site not read again so soon, a
 * failure, a part read; or what a discovery check found.
 */
export function siteText(line: ScanLine, locale: string, t: TFunction<"sources">): string {
  switch (line.outcome) {
    case "checked": {
      return t("scanned.site.checked", { note: line.note ?? "" });
    }
    case "error": {
      return t("scanned.site.error", { error: errorText(line.error_kind, t) });
    }
    case "skipped_poll_limit":
    case "read": {
      const counts = t("scanned.site.read", {
        read: formatCount(line.jobs_read, locale),
        quick: formatCount(line.quick_looked, locale),
        full: formatCount(line.scored_by_ai, locale),
        matched: formatCount(line.matched, locale),
        cost: costText(line.ai_cost_eur, locale, t),
      });
      const partial = PARTIAL.exec(line.note ?? "");
      const notes = [
        line.outcome === "skipped_poll_limit" ? t("scanned.site.skipped") : null,
        partial === null
          ? null
          : t("scanned.site.partial", {
              count: Number(partial[1] ?? "0"),
              page: partial[2] ?? "",
            }),
      ].filter((note) => note !== null);
      return [counts, ...notes].join(" ");
    }
  }
}

/** What one search or discovery run did, in full. */
function RunDetail({ run }: { run: ScanRun }) {
  const { t, i18n } = useTranslation("sources");
  const [bySite, setBySite] = useState(false);
  const locale = i18n.language;
  const funnel = run.funnel ?? null;
  const sites = (
    <PropertyList testId="scanned-sites">
      {run.lines.map((line) => (
        <Property key={line.board_id} label={line.board_name}>
          <Tagged testId="scanned-line">
            {line.outcome === "error" ? (
              <StatusText tone="problem">{siteText(line, locale, t)}</StatusText>
            ) : (
              <Text tone="secondary">{siteText(line, locale, t)}</Text>
            )}
          </Tagged>
        </Property>
      ))}
    </PropertyList>
  );
  if (funnel === null) {
    // A discovery run: what it checked.
    return (
      <Stack gap="sm" testId="scanned-run">
        <Heading level={2} size="pane">
          {runTitle(run, locale, t)}
        </Heading>
        {sites}
      </Stack>
    );
  }
  const left = leftOutLines(funnel, locale, t);
  const matches = matchLines(run.lines, locale, t);
  return (
    <Stack gap="md" testId="scanned-run">
      <Heading level={2} size="pane">
        {runTitle(run, locale, t)}
      </Heading>
      <Figures items={runFigures(run, funnel, locale, t)} testId="scanned-figures" />
      {funnel.from_sites === null || funnel.from_sites === undefined ? null : (
        <Tagged testId="scanned-from-store">
          <Text tone="secondary">
            {t("scanned.fromStore", {
              sites: t("scanned.fromSites", { count: funnel.from_sites }),
              pages: t("scanned.fromPages", { count: funnel.from_company_pages ?? 0 }),
            })}
          </Text>
        </Tagged>
      )}
      {funnel.stopped === null || funnel.stopped === undefined ? null : (
        <Tagged testId="scanned-stopped">
          <Text tone="secondary">{t(`stopped.${funnel.stopped}`)}</Text>
        </Tagged>
      )}
      <Stack gap="sm">
        <Heading level={3} size="section">
          {t("scanned.from")}
        </Heading>
        {matches.length === 0 ? (
          <Text tone="secondary">{t("scanned.noMatches")}</Text>
        ) : (
          <Bullets items={matches} testId="scanned-from" />
        )}
      </Stack>
      {left.length === 0 ? null : (
        <Stack gap="sm">
          <Heading level={3} size="section">
            {t("scanned.leftOut")}
          </Heading>
          <Bullets items={left} testId="scanned-left-out" />
        </Stack>
      )}
      {funnel.kept_with_note > 0 ? (
        <Tagged testId="scanned-kept-with-note">
          <Text tone="secondary">
            {t("scanned.keptWithNote", {
              count: funnel.kept_with_note,
              total: formatCount(funnel.read_in_full, locale),
            })}
          </Text>
        </Tagged>
      ) : null}
      {run.lines.length === 0 ? null : (
        <Stack gap="sm">
          <Cluster>
            <Button
              variant="secondary"
              aria-expanded={bySite}
              data-testid="scanned-by-site"
              onClick={() => {
                setBySite((open) => !open);
              }}
            >
              {bySite ? t("scanned.hideBySite") : t("scanned.bySite")}
            </Button>
          </Cluster>
          {bySite ? sites : null}
        </Stack>
      )}
    </Stack>
  );
}

/** The figures of a run in its row: a search's four numbers, or what discovery checked. */
function rowMeta(run: ScanRun, locale: string, t: TFunction<"sources">): ReactNode {
  const funnel = run.funnel ?? null;
  if (funnel === null) {
    return <Text tone="secondary">{t("scanned.sitesChecked", { count: run.lines.length })}</Text>;
  }
  return <Figures compact items={runFigures(run, funnel, locale, t)} />;
}

/** The line at the bottom: when the app searches, Search now, and about what one costs. */
function SearchNowLine() {
  const { t, i18n } = useTranslation("sources");
  const estimate = useEstimate();
  const latest = useLatestSearch();
  const cost = estimate.data?.cost_eur ?? 0;
  return (
    <Stack gap="sm" testId="scanned-search-now">
      <Text tone="secondary">{t("scanned.nightlyLine")}</Text>
      {isRunning(latest.data) ? (
        <Tagged testId="scanned-running">
          <Text tone="secondary">{t("scanned.running")}</Text>
        </Tagged>
      ) : null}
      <Cluster>
        <SearchNowButton variant="secondary" testId="scanned-search" />
      </Cluster>
      {cost > 0 ? (
        <Tagged testId="scanned-search-cost">
          <Text tone="secondary">
            {cost < 0.01
              ? t("scanned.aboutUnder", { cost: formatEuros(0.01, i18n.language) })
              : t("scanned.about", { cost: formatEuros(cost, i18n.language) })}
          </Text>
        </Tagged>
      ) : null}
    </Stack>
  );
}

/**
 * The Searches tab: every search with its four numbers (jobs read, read in
 * full, good matches, cost); the latest open, with where the matches came
 * from and why the other jobs were left out (counts that add up to the jobs
 * read), and its sites behind "Show by site". Earlier searches and discovery
 * runs are one row each; Details opens one. Search now at the bottom, its
 * cost on the line under it.
 */
export function ScannedView() {
  const { t, i18n } = useTranslation("sources");
  const scans = useScans(null);
  const latest = useLatestSearch();
  const [chosen, setChosen] = useState<string | null>(null);
  const runs = scans.data?.pages.flatMap((page) => page.runs) ?? [];
  const running = isRunning(latest.data);
  const wasRunning = useRef(running);
  const refetch = scans.refetch;
  useEffect(() => {
    // A search just ended: show it.
    if (wasRunning.current && !running) {
      void refetch();
    }
    wasRunning.current = running;
  }, [running, refetch]);

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
    // The newest search opens, not a discovery run that came after it.
    const open =
      runs.find((run) => run.id === chosen) ?? runs.find((run) => run.kind === "search") ?? runs[0];
    const others = runs.filter((run) => run.id !== open?.id);
    body = (
      <Stack gap="lg">
        {open === undefined ? null : <RunDetail key={open.id} run={open} />}
        {others.length === 0 ? null : (
          <Stack gap="sm">
            <Heading level={2} size="section">
              {t("scanned.earlier")}
            </Heading>
            <RowList label={t("scanned.earlier")} testId="scanned-earlier">
              {others.map((run) => (
                <RowItem
                  key={run.id}
                  testId="scanned-earlier-run"
                  title={runTitle(run, i18n.language, t)}
                  meta={rowMeta(run, i18n.language, t)}
                  actions={
                    <Button
                      variant="secondary"
                      data-testid="scanned-details"
                      onClick={() => {
                        setChosen(run.id);
                      }}
                    >
                      {t("scanned.details")}
                    </Button>
                  }
                />
              ))}
            </RowList>
          </Stack>
        )}
        {scans.hasNextPage ? (
          <Cluster>
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
          </Cluster>
        ) : null}
      </Stack>
    );
  }
  return (
    <TouchTargets>
      <Stack gap="lg">
        <Text tone="secondary">{t("scanned.lead")}</Text>
        {body}
        <SearchNowLine />
      </Stack>
    </TouchTargets>
  );
}
