import type { TFunction } from "i18next";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  DataTable,
  Fact,
  FactList,
  Placeholder,
  RowItem,
  RowList,
  Section,
  Select,
  SkeletonList,
  Stack,
  Tag,
  Tagged,
  Text,
  TextField,
  useNarrowScreen,
  type DataColumn,
} from "~/lib/ui";

import { useAdminOverview, useReopen, withFinder, type AdminOverview } from "../../api/admin";
import {
  adminRows,
  filterRows,
  NO_FILTERS,
  SHOW_OPTIONS,
  type AdminFilters,
  type AdminRow,
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
 * The Job sites page for board owners: totals, what happened this week, the boards that wait for
 * you with a plain reason, and every board with a search, filters and a side panel.
 */
export function BoardsAdmin({ back }: BoardsAdminProps) {
  const { t } = useTranslation("sources");
  const boards = useBoards();
  const overview = useAdminOverview(boards.isSuccess && boards.data.can_manage);
  const [open, setOpen] = useState<string | null>(null);
  // Boards that waited for you when you opened them: one you decline stays in the list.
  const [pinned, setPinned] = useState<ReadonlySet<string>>(new Set());
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
                size="lg"
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
  const show = (row: AdminRow) => {
    if (row.status === "waiting") {
      setPinned((current) => new Set(current).add(row.board.id));
    }
    setOpen(row.board.id);
  };
  const opened = rows.find((row) => row.board.id === open);
  return (
    <Stack gap="lg">
      {back}
      <Totals overview={overview.data} />
      <ThisWeek overview={overview.data} />
      <Waiting
        rows={rows.filter(
          (row) =>
            row.status === "waiting" ||
            (pinned.has(row.board.id) && row.board.status === "declined"),
        )}
        onOpen={show}
      />
      <AllBoards rows={rows} selected={open} onOpen={show} />
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

/** One board waiting for you; after you decline it, it stays here with "Declined" and Undo. */
function WaitingRow({ row, onOpen }: { row: AdminRow; onOpen: (row: AdminRow) => void }) {
  const { t, i18n } = useTranslation("sources");
  const reopen = useReopen(row.board.id);
  if (row.board.status === "declined") {
    return (
      <RowItem
        testId="boards-admin-waiting-row"
        title={row.board.name}
        meta={
          <Tagged testId="boards-admin-declined">
            <Tag tone="danger">{t("admin.declined")}</Tag>
          </Tagged>
        }
        actions={
          <Button
            variant="secondary"
            size="lg"
            disabled={reopen.isPending}
            data-testid="boards-admin-undo"
            onClick={() => {
              reopen.mutate();
            }}
          >
            {t("admin.undo")}
          </Button>
        }
      />
    );
  }
  return (
    <RowItem
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
          size="lg"
          data-testid="boards-admin-waiting-open"
          onClick={() => {
            onOpen(row);
          }}
        >
          {t(`admin.cta.${row.need ?? "approve"}`)}
        </Button>
      }
    />
  );
}

function Waiting({ rows, onOpen }: { rows: AdminRow[]; onOpen: (row: AdminRow) => void }) {
  const { t } = useTranslation("sources");
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
              <WaitingRow key={row.board.id} row={row} onOpen={onOpen} />
            ))}
          </RowList>
          {rows.length > shown.length ? (
            <Button
              variant="ghost"
              size="lg"
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
  const set = (change: Partial<AdminFilters>) => {
    setFilters((current) => ({ ...current, ...change }));
  };
  const columns = boardColumns(t, i18n.language);
  const narrow = useNarrowScreen();
  return (
    <Section title={t("admin.all.title", { total: rows.length })} testId="boards-admin-all">
      <Stack gap="sm">
        <TextField
          label={t("admin.all.search")}
          type="search"
          size="lg"
          autoComplete="off"
          placeholder={t("admin.all.searchHint")}
          value={filters.search}
          onChange={(event) => {
            set({ search: event.target.value });
          }}
          data-testid="boards-admin-search"
        />
        <Select
          label={t("admin.filters.show")}
          size="lg"
          options={SHOW_OPTIONS.map((value) => ({
            value,
            label: t(`admin.filters.options.${value}`),
          }))}
          value={filters.show}
          onChange={(show) => {
            set({ show });
          }}
          data-testid="boards-admin-show"
        />
      </Stack>
      {matching.length === 0 ? (
        <Placeholder
          testId="boards-admin-all-empty"
          icon="search"
          title={t("admin.all.emptyTitle")}
          action={
            <Button
              variant="secondary"
              size="lg"
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
          {narrow ? (
            <RowList label={t("admin.all.table")} testId="boards-admin-table">
              {shown.map((row) => (
                <RowItem
                  key={row.board.id}
                  testId="boards-admin-row"
                  title={row.board.name}
                  subtitle={t(`admin.how.${row.board.kind}`)}
                  meta={
                    <>
                      <StageLabel status={row.status} />
                      <Text tone="secondary">{row.board.country ?? t("admin.worldwide")}</Text>
                    </>
                  }
                  actions={
                    <Button
                      variant="secondary"
                      size="lg"
                      data-testid="boards-admin-open"
                      onClick={() => {
                        onOpen(row);
                      }}
                    >
                      {t("admin.open")}
                    </Button>
                  }
                />
              ))}
            </RowList>
          ) : (
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
          )}
          <Text tone="secondary" size="sm">
            {t("admin.all.showing", { shown: shown.length, total: rows.length })}
          </Text>
          {matching.length > shown.length ? (
            <Button
              variant="ghost"
              size="lg"
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
