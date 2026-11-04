import type { TFunction } from "i18next";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Chip,
  ChipRow,
  DataTable,
  Fact,
  FactList,
  Placeholder,
  RowItem,
  RowList,
  Section,
  SkeletonList,
  Stack,
  Tagged,
  Text,
  TextField,
  type DataColumn,
} from "~/lib/ui";

import { useAdminOverview, withFinder, type AdminOverview } from "../../api/admin";
import {
  adminRows,
  filterRows,
  NO_FILTERS,
  topTags,
  type AdminFilters,
  type AdminRow,
  type RowKind,
  type RowStatus,
} from "../../api/admin-rows";
import { useBoards } from "../../api/boards";
import { shortDate } from "../../api/format";
import { AdminPeek } from "./AdminPeek";
import { StageLabel } from "./StageLabel";

/** Rows of the waiting list and of the table shown before "Show more". */
const WAITING_SHOWN = 8;
const TABLE_SHOWN = 100;

export interface BoardsAdminProps {
  /** The way back to You, above the page. */
  back: ReactNode;
}

/**
 * The Job boards page for board owners: totals, what happened this week, the boards that wait for
 * you with a plain reason, and every board with a search, filters and a side panel.
 */
export function BoardsAdmin({ back }: BoardsAdminProps) {
  const { t } = useTranslation("sources");
  const boards = useBoards();
  const overview = useAdminOverview(boards.isSuccess && boards.data.can_manage);
  const [open, setOpen] = useState<string | null>(null);
  if (boards.isPending || (boards.isSuccess && boards.data.can_manage && overview.isPending)) {
    return (
      <Stack gap="lg">
        {back}
        <Tagged testId="boards-admin-loading">
          <SkeletonList rows={6} />
        </Tagged>
      </Stack>
    );
  }
  if (boards.isError || overview.isError || overview.data === undefined) {
    return (
      <Stack gap="lg">
        {back}
        <Tagged testId="boards-admin-error">
          <Banner
            tone="danger"
            icon="warning"
            title={t("admin.loadFailed")}
            action={
              <Button
                variant="secondary"
                onClick={() => {
                  void boards.refetch();
                  void overview.refetch();
                }}
              >
                {t("admin.retry")}
              </Button>
            }
          />
        </Tagged>
      </Stack>
    );
  }
  const rows = adminRows(boards.data.boards, overview.data);
  const opened = rows.find((row) => row.board.id === open);
  return (
    <Stack gap="lg">
      {back}
      <Totals overview={overview.data} />
      <ThisWeek overview={overview.data} />
      <Waiting
        rows={rows.filter((row) => row.status === "waiting")}
        onOpen={(row) => {
          setOpen(row.board.id);
        }}
      />
      <AllBoards
        rows={rows}
        selected={open}
        onOpen={(row) => {
          setOpen(row.board.id);
        }}
      />
      <AdminPeek
        row={opened}
        onClose={() => {
          setOpen(null);
        }}
      />
    </Stack>
  );
}

function Totals({ overview }: { overview: AdminOverview }) {
  const { t, i18n } = useTranslation("sources");
  const n = (value: number) => new Intl.NumberFormat(i18n.language).format(value);
  const { totals } = overview;
  const reading = totals.reading_sites + totals.reading_companies;
  return (
    <Tagged testId="boards-admin-totals">
      <FactList>
        <Fact
          label={t("admin.totals.reading")}
          detail={t("admin.totals.readingDetail", {
            sites: n(totals.reading_sites),
            companies: n(totals.reading_companies),
          })}
        >
          <Tagged testId="boards-admin-total-reading">{n(reading)}</Tagged>
        </Fact>
        <Fact label={t("admin.totals.waiting")} detail={t("admin.totals.waitingDetail")}>
          <Tagged testId="boards-admin-total-waiting">{n(totals.waiting)}</Tagged>
        </Fact>
        <Fact label={t("admin.totals.checking")} detail={t("admin.totals.checkingDetail")}>
          <Tagged testId="boards-admin-total-checking">{n(totals.checking)}</Tagged>
        </Fact>
        <Fact label={t("admin.totals.notAllowed")} detail={t("admin.totals.notAllowedDetail")}>
          <Tagged testId="boards-admin-total-notAllowed">{n(totals.not_allowed)}</Tagged>
        </Fact>
        <Fact label={t("admin.totals.paused")} detail={t("admin.totals.pausedDetail")}>
          <Tagged testId="boards-admin-total-paused">{n(totals.paused)}</Tagged>
        </Fact>
      </FactList>
    </Tagged>
  );
}

/** What happened in the last 7 days, only from what the app recorded. */
function ThisWeek({ overview }: { overview: AdminOverview }) {
  const { t } = useTranslation("sources");
  const week = overview.this_week;
  const lines = [
    week.found > 0 || week.checked > 0
      ? t("admin.week.found", { count: week.found, checked: week.checked })
      : null,
    week.started_reading > 0 ? t("admin.week.started", { count: week.started_reading }) : null,
    week.rechecked > 0 ? t("admin.week.rechecked", { count: week.rechecked }) : null,
  ].filter((line): line is string => line !== null);
  return (
    <Section title={t("admin.week.title")} testId="boards-admin-week">
      <Stack gap="sm">
        {lines.length === 0 ? (
          <Text tone="secondary">{t("admin.week.nothing")}</Text>
        ) : (
          lines.map((line) => <Text key={line}>{line}</Text>)
        )}
      </Stack>
    </Section>
  );
}

function Waiting({ rows, onOpen }: { rows: AdminRow[]; onOpen: (row: AdminRow) => void }) {
  const { t, i18n } = useTranslation("sources");
  const [all, setAll] = useState(false);
  const shown = all ? rows : rows.slice(0, WAITING_SHOWN);
  return (
    <Section
      title={t("admin.waiting.title")}
      description={t("admin.waiting.description")}
      testId="boards-admin-waiting"
    >
      {rows.length === 0 ? (
        <Placeholder
          testId="boards-admin-waiting-empty"
          icon="check"
          title={t("admin.waiting.emptyTitle")}
        >
          {t("admin.waiting.emptyBody")}
        </Placeholder>
      ) : (
        <Stack gap="sm">
          <RowList label={t("admin.waiting.title")} testId="boards-admin-waiting-list">
            {shown.map((row) => (
              <RowItem
                key={row.board.id}
                testId="boards-admin-waiting-row"
                title={row.board.name}
                subtitle={t(`admin.need.${row.need ?? "approve"}`)}
                meta={
                  <>
                    <StageLabel status="waiting" />
                    {row.askedAt === undefined ? null : (
                      <Text tone="secondary">
                        {t("admin.askedOn", { date: shortDate(row.askedAt, i18n.language) })}
                      </Text>
                    )}
                  </>
                }
                actions={
                  <Button
                    variant="secondary"
                    data-testid="boards-admin-waiting-open"
                    onClick={() => {
                      onOpen(row);
                    }}
                  >
                    {t(`admin.cta.${row.need ?? "approve"}`)}
                  </Button>
                }
              />
            ))}
          </RowList>
          {rows.length > shown.length ? (
            <Button
              variant="ghost"
              data-testid="boards-admin-waiting-more"
              onClick={() => {
                setAll(true);
              }}
            >
              {t("admin.showMore", { count: rows.length - shown.length })}
            </Button>
          ) : null}
        </Stack>
      )}
    </Section>
  );
}

const KINDS: readonly (RowKind | "all")[] = ["all", "site", "company"];
const STATUSES: readonly (RowStatus | "all")[] = [
  "all",
  "reading",
  "waiting",
  "checking",
  "notAllowed",
  "paused",
];

function AllBoards({
  rows,
  selected,
  onOpen,
}: {
  rows: AdminRow[];
  selected: string | null;
  onOpen: (row: AdminRow) => void;
}) {
  const { t, i18n } = useTranslation("sources");
  const [filters, setFilters] = useState<AdminFilters>(NO_FILTERS);
  const [all, setAll] = useState(false);
  const matching = filterRows(rows, filters);
  const shown = all ? matching : matching.slice(0, TABLE_SHOWN);
  const tags = topTags(rows);
  const set = (change: Partial<AdminFilters>) => {
    setFilters((current) => ({ ...current, ...change }));
  };
  const columns = boardColumns(t, i18n.language);
  return (
    <Section title={t("admin.all.title", { total: rows.length })} testId="boards-admin-all">
      <Stack gap="sm">
        <TextField
          label={t("admin.all.search")}
          type="search"
          autoComplete="off"
          placeholder={t("admin.all.searchHint")}
          value={filters.search}
          onChange={(event) => {
            set({ search: event.target.value });
          }}
          data-testid="boards-admin-search"
        />
        <ChipRow label={t("admin.filters.kind")}>
          {KINDS.map((kind) => (
            <Chip
              key={kind}
              pressed={filters.kind === kind}
              data-testid={`boards-admin-kind-${kind}`}
              onClick={() => {
                set({ kind });
              }}
            >
              {t(`admin.filters.kinds.${kind}`)}
            </Chip>
          ))}
        </ChipRow>
        <ChipRow label={t("admin.filters.status")}>
          {STATUSES.map((status) => (
            <Chip
              key={status}
              pressed={filters.status === status}
              data-testid={`boards-admin-filter-${status}`}
              onClick={() => {
                set({ status });
              }}
            >
              {status === "all" ? t("admin.filters.anyStatus") : t(`admin.status.${status}`)}
            </Chip>
          ))}
        </ChipRow>
        {tags.length === 0 ? null : (
          <ChipRow label={t("admin.filters.jobs")}>
            {["all", ...tags].map((tag) => (
              <Chip
                key={tag}
                pressed={filters.tag === tag}
                data-testid={`boards-admin-tag-${tag}`}
                onClick={() => {
                  set({ tag });
                }}
              >
                {tag === "all" ? t("admin.filters.allJobs") : tag}
              </Chip>
            ))}
          </ChipRow>
        )}
      </Stack>
      {matching.length === 0 ? (
        <Placeholder
          testId="boards-admin-all-empty"
          icon="search"
          title={t("admin.all.emptyTitle")}
          action={
            <Button
              variant="secondary"
              data-testid="boards-admin-clear"
              onClick={() => {
                setFilters(NO_FILTERS);
              }}
            >
              {t("admin.all.clear")}
            </Button>
          }
        >
          {rows.length === 0 ? t("admin.all.emptyNone") : t("admin.all.emptyBody")}
        </Placeholder>
      ) : (
        <Stack gap="sm">
          <DataTable
            label={t("admin.all.table")}
            columns={columns}
            rows={shown}
            rowKey={(row) => row.board.id}
            selected={selected}
            onOpen={onOpen}
            rowTestId="boards-admin-row"
            openTestId="boards-admin-open"
            testId="boards-admin-table"
          />
          <Text tone="secondary" size="sm">
            {t("admin.all.showing", { shown: shown.length, total: rows.length })}
          </Text>
          {matching.length > shown.length ? (
            <Button
              variant="ghost"
              data-testid="boards-admin-more"
              onClick={() => {
                setAll(true);
              }}
            >
              {t("admin.showMore", { count: matching.length - shown.length })}
            </Button>
          ) : null}
        </Stack>
      )}
    </Section>
  );
}

/** The table's columns: the board first (it opens the board), then what the owner scans for. */
function boardColumns(t: TFunction<"sources">, locale: string): DataColumn<AdminRow>[] {
  return [
    {
      id: "board",
      header: t("admin.table.board"),
      cell: (row) => (
        <>
          <Text as="span" weight="medium">
            {row.board.name}
          </Text>
          <Text as="span" tone="secondary" size="sm">
            {` · ${t(`admin.how.${row.board.kind}`)}`}
          </Text>
        </>
      ),
    },
    {
      id: "jobs",
      header: t("admin.table.jobs"),
      cell: (row) => (
        <Text numeric>
          {row.board.last_fetch_jobs === undefined || row.board.last_fetch_jobs === null
            ? t("admin.none")
            : new Intl.NumberFormat(locale).format(row.board.last_fetch_jobs)}
        </Text>
      ),
      nowrap: true,
    },
    {
      id: "where",
      header: t("admin.table.where"),
      cell: (row) => <Text>{row.board.country ?? t("admin.worldwide")}</Text>,
    },
    {
      id: "status",
      header: t("admin.table.status"),
      cell: (row) => <StageLabel status={row.status} />,
      nowrap: true,
    },
    {
      id: "found",
      header: t("admin.table.found"),
      cell: (row) => <Text tone="secondary">{foundThrough(row, t)}</Text>,
    },
  ];
}

/** Where the app first heard of a board, in plain words. */
function foundThrough(row: AdminRow, t: TFunction<"sources">): string {
  const finder = withFinder(row.board).found_through;
  if (finder !== undefined && finder !== null && finder.trim() !== "") {
    return finder;
  }
  return t(`admin.origin.${row.board.origin}`);
}
