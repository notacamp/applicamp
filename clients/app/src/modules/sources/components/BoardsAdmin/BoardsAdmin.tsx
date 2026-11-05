import type { TFunction } from "i18next";
import { useEffect, useState, type ReactNode } from "react";
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

import {
  howRead,
  useAdminList,
  useAdminOverview,
  useReopen,
  type AdminListState,
  type AdminOverview,
} from "../../api/admin";
import { SHOW_OPTIONS, type AdminRow, type ShowFilter } from "../../api/admin-rows";
import { shortDate } from "../../api/format";
import { AdminPeek } from "./AdminPeek";
import { StageLabel } from "./StageLabel";

/** Boards of the waiting list shown before "Show more". */
const WAITING_SHOWN = 8;
/** How long the search waits for the typing to stop. */
const TYPING_MS = 250;

export interface BoardsAdminProps {
  /** The way back to You, above the page. */
  back: ReactNode;
}

/** A board you opened while it waited for you, and where it stood in that list. */
interface Seen {
  row: AdminRow;
  index: number;
}

/**
 * The Job sites page for board owners: totals, what happened this week, the boards that wait for
 * you with a plain reason, and every board with a search, a Show select and a side panel. The list
 * is searched, filtered and paged by the server.
 */
export function BoardsAdmin({ back }: BoardsAdminProps) {
  const { t } = useTranslation("sources");
  const overview = useAdminOverview(true);
  const [open, setOpen] = useState<AdminRow | null>(null);
  const [seen, setSeen] = useState<ReadonlyMap<string, Seen>>(new Map());
  // Boards you declined from the waiting list: each stays in its place, with Undo.
  const [declined, setDeclined] = useState<ReadonlySet<string>>(new Set());
  const [search, setSearch] = useState("");
  const [show, setShow] = useState<ShowFilter>("all");
  const q = useDebounced(search);
  const waiting = useAdminList("waiting", "");
  const table = useAdminList(show, q);
  if (overview.isPending) {
    return (
      <Stack gap="lg">
        {back}
        <Tagged testId="boards-admin-loading">
          <SkeletonList rows={6} />
        </Tagged>
      </Stack>
    );
  }
  if (overview.isError) {
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
  const waitingRows = waiting.rows;
  const openRow = (row: AdminRow, index: number) => {
    if (row.status === "waiting") {
      setSeen((current) => new Map(current).set(row.board.id, { row, index }));
    }
    setOpen(row);
  };
  const latest =
    open === null
      ? undefined
      : ([...waitingRows, ...table.rows].find((row) => row.board.id === open.board.id) ?? open);
  return (
    <Stack gap="lg">
      {back}
      <Totals overview={overview.data} />
      <ThisWeek overview={overview.data} />
      <Waiting
        query={waiting}
        declined={declined}
        seen={seen}
        onOpen={openRow}
        onUndone={(id) => {
          setDeclined((current) => {
            const next = new Set(current);
            next.delete(id);
            return next;
          });
        }}
      />
      <AllBoards
        query={table}
        search={search}
        show={show}
        onSearch={setSearch}
        onShow={setShow}
        selected={open?.board.id ?? null}
        onOpen={(row) => {
          openRow(row, 0);
        }}
      />
      <AdminPeek
        row={latest}
        onClose={() => {
          setOpen(null);
        }}
        onDeclined={(id) => {
          if (seen.has(id)) {
            setDeclined((current) => new Set(current).add(id));
          }
        }}
      />
    </Stack>
  );
}

/** `value` once it has stopped changing for a moment. */
function useDebounced(value: string): string {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => {
      setSettled(value);
    }, TYPING_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [value]);
  return settled;
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
function WaitingRow({
  row,
  declined,
  onOpen,
  onUndone,
}: {
  row: AdminRow;
  declined: boolean;
  onOpen: (row: AdminRow) => void;
  onUndone: (id: string) => void;
}) {
  const { t, i18n } = useTranslation("sources");
  const reopen = useReopen(row.board.id);
  if (declined) {
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
            disabled={reopen.isPending}
            data-testid="boards-admin-undo"
            onClick={() => {
              reopen.mutate(undefined, {
                onSuccess: () => {
                  onUndone(row.board.id);
                },
              });
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

/** The boards that wait for you; one you declined stays where it was, with Undo. */
function Waiting({
  query,
  declined,
  seen,
  onOpen,
  onUndone,
}: {
  query: AdminListState;
  declined: ReadonlySet<string>;
  seen: ReadonlyMap<string, Seen>;
  onOpen: (row: AdminRow, index: number) => void;
  onUndone: (id: string) => void;
}) {
  const { t } = useTranslation("sources");
  const [all, setAll] = useState(false);
  const rows = [...query.rows];
  for (const id of declined) {
    const was = seen.get(id);
    if (was !== undefined && !rows.some((row) => row.board.id === id)) {
      rows.splice(Math.min(was.index, rows.length), 0, was.row);
    }
  }
  const shown = all ? rows : rows.slice(0, WAITING_SHOWN);
  return (
    <Section
      title={t("admin.waiting.title")}
      description={t("admin.waiting.description")}
      testId="boards-admin-waiting"
    >
      {query.isPending ? (
        <SkeletonList rows={3} />
      ) : rows.length === 0 ? (
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
            {shown.map((row, index) => (
              <WaitingRow
                key={row.board.id}
                row={row}
                declined={declined.has(row.board.id)}
                onOpen={(opened) => {
                  onOpen(opened, index);
                }}
                onUndone={onUndone}
              />
            ))}
          </RowList>
          {rows.length > shown.length || query.hasNextPage ? (
            <Button
              variant="ghost"
              data-testid="boards-admin-waiting-more"
              disabled={query.isFetchingNextPage}
              onClick={() => {
                setAll(true);
                if (rows.length <= shown.length) {
                  void query.fetchNextPage();
                }
              }}
            >
              {t("admin.more")}
            </Button>
          ) : null}
        </Stack>
      )}
    </Section>
  );
}

function AllBoards({
  query,
  search,
  show,
  onSearch,
  onShow,
  selected,
  onOpen,
}: {
  query: AdminListState;
  search: string;
  show: ShowFilter;
  onSearch: (search: string) => void;
  onShow: (show: ShowFilter) => void;
  selected: string | null;
  onOpen: (row: AdminRow) => void;
}) {
  const { t, i18n } = useTranslation("sources");
  const narrow = useNarrowScreen();
  const columns = boardColumns(t, i18n.language);
  const rows = query.rows;
  const filtered = search.trim() !== "" || show !== "all";
  return (
    <Section title={t("admin.all.title")} testId="boards-admin-all">
      <Stack gap="sm">
        <TextField
          label={t("admin.all.search")}
          type="search"
          autoComplete="off"
          placeholder={t("admin.all.searchHint")}
          value={search}
          onChange={(event) => {
            onSearch(event.target.value);
          }}
          data-testid="boards-admin-search"
        />
        <Select
          label={t("admin.filters.show")}
          options={SHOW_OPTIONS.map((value) => ({
            value,
            label: t(`admin.filters.options.${value}`),
          }))}
          value={show}
          onChange={onShow}
          data-testid="boards-admin-show"
        />
      </Stack>
      {query.isPending ? (
        <SkeletonList rows={6} />
      ) : query.isError ? (
        <Tagged testId="boards-admin-table-error">
          <Banner
            tone="danger"
            icon="warning"
            title={t("admin.loadFailed")}
            action={
              <Button
                variant="secondary"
                onClick={() => {
                  void query.refetch();
                }}
              >
                {t("admin.retry")}
              </Button>
            }
          />
        </Tagged>
      ) : rows.length === 0 ? (
        <Placeholder
          testId="boards-admin-all-empty"
          icon="search"
          title={t("admin.all.emptyTitle")}
          action={
            filtered ? (
              <Button
                variant="secondary"
                data-testid="boards-admin-clear"
                onClick={() => {
                  onSearch("");
                  onShow("all");
                }}
              >
                {t("admin.all.clear")}
              </Button>
            ) : undefined
          }
        >
          {filtered ? t("admin.all.emptyBody") : t("admin.all.emptyNone")}
        </Placeholder>
      ) : (
        <Stack gap="sm">
          {narrow ? (
            <RowList label={t("admin.all.table")} testId="boards-admin-table">
              {rows.map((row) => (
                <RowItem
                  key={row.board.id}
                  testId="boards-admin-row"
                  title={row.board.name}
                  subtitle={howRead(row.board.kind, t)}
                  meta={
                    <>
                      <StageLabel status={row.status} />
                      <Text tone="secondary">{row.board.country ?? t("admin.worldwide")}</Text>
                    </>
                  }
                  actions={
                    <Button
                      variant="secondary"
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
              rows={rows}
              rowKey={(row) => row.board.id}
              selected={selected}
              onOpen={onOpen}
              rowTestId="boards-admin-row"
              openTestId="boards-admin-open"
              testId="boards-admin-table"
            />
          )}
          <Text tone="secondary" size="sm">
            {t("admin.all.showing", { shown: rows.length })}
          </Text>
          {query.hasNextPage ? (
            <Button
              variant="ghost"
              data-testid="boards-admin-more"
              disabled={query.isFetchingNextPage}
              onClick={() => {
                void query.fetchNextPage();
              }}
            >
              {t("admin.more")}
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
            {` · ${howRead(row.board.kind, t)}`}
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
  const finder = row.board.found_through;
  if (finder !== undefined && finder !== null && finder.trim() !== "") {
    return finder;
  }
  return t(`admin.origin.${row.board.origin}`);
}
