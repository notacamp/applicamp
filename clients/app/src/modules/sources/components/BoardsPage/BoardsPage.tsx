import type { TFunction } from "i18next";
import { useState, type ReactNode, type SubmitEvent } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  BoardCard,
  Button,
  CardList,
  Cluster,
  Label,
  Section,
  Stack,
  Tagged,
  Text,
  TextField,
  ViewSwitch,
} from "~/lib/ui";

import {
  boardErrorOf,
  salaryLine,
  skillsFitLine,
  useBoards,
  useSuggestBoard,
  type BoardStatus,
  type BoardSummary,
} from "../../api/boards";
import { formatWhen } from "../../api/format";
import { BoardPeek } from "../BoardPeek/BoardPeek";
import { ScannedView } from "../ScannedView/ScannedView";

/** The Boards page's tabs. */
export type BoardsTab = "boards" | "scanned";

export interface BoardsPageProps {
  tab: BoardsTab;
  onTab: (tab: BoardsTab) => void;
}

/**
 * The job boards: the approved ones a search reads, the ones waiting for the
 * owner (with their automatic check), and the declined ones, each list
 * ordered by average salary; "Suggest a board"; and the Scanned tab, what
 * each search and discovery run scanned.
 */
export function BoardsPage({ tab, onTab }: BoardsPageProps) {
  const { t } = useTranslation("sources");
  return (
    <Stack>
      <ViewSwitch
        label={t("boards.tabs.label")}
        options={[
          {
            value: "boards" as const,
            label: t("boards.tabs.boards"),
            icon: "bullets",
            testId: "boards-tab-boards",
          },
          {
            value: "scanned" as const,
            label: t("boards.tabs.scanned"),
            icon: "search",
            testId: "boards-tab-scanned",
          },
        ]}
        value={tab}
        onChange={onTab}
      />
      {tab === "boards" ? <BoardsList /> : <ScannedView />}
    </Stack>
  );
}

/** Each status's section, in the page's order. */
const SECTIONS = [
  { id: "approved", statuses: ["approved", "paused"] },
  { id: "waiting", statuses: ["candidate"] },
  { id: "declined", statuses: ["declined"] },
] as const satisfies readonly { id: string; statuses: readonly BoardStatus[] }[];

function BoardsList() {
  const { t } = useTranslation("sources");
  const boards = useBoards();
  const [open, setOpen] = useState<string | null>(null);
  if (boards.isPending) {
    return (
      <Tagged testId="boards-loading">
        <Text tone="secondary">{t("boards.loading")}</Text>
      </Tagged>
    );
  }
  if (boards.isError) {
    return (
      <Tagged testId="boards-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("boards.loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void boards.refetch();
              }}
            >
              {t("boards.retry")}
            </Button>
          }
        />
      </Tagged>
    );
  }
  const all = boards.data.boards;
  return (
    <Stack gap="lg">
      <SuggestBoard />
      {SECTIONS.map((section) => {
        const statuses: readonly BoardStatus[] = section.statuses;
        const rows = all.filter((board) => statuses.includes(board.status));
        return (
          <Section
            key={section.id}
            title={t(`boards.sections.${section.id}.title`)}
            description={t(`boards.sections.${section.id}.description`)}
            testId={`boards-section-${section.id}`}
          >
            {rows.length === 0 ? (
              <Text tone="secondary">{t(`boards.sections.${section.id}.empty`)}</Text>
            ) : (
              <CardList label={t(`boards.sections.${section.id}.title`)}>
                {rows.map((board) => (
                  <BoardRow
                    key={board.id}
                    board={board}
                    onOpen={() => {
                      setOpen(board.id);
                    }}
                  />
                ))}
              </CardList>
            )}
          </Section>
        );
      })}
      <BoardPeek
        id={open}
        onClose={() => {
          setOpen(null);
        }}
      />
    </Stack>
  );
}

/** One board: its name, its salary line, its labels and what happened lately. */
function BoardRow({ board, onOpen }: { board: BoardSummary; onOpen: () => void }) {
  const { t, i18n } = useTranslation("sources");
  const salary = salaryLine(board.ranking, i18n.language, t);
  const fit = skillsFitLine(board, t);
  const meta: ReactNode[] = [];
  if (board.status === "paused") {
    meta.push(
      <Label key="paused" hue="yellow">
        {t("boards.status.paused")}
      </Label>,
    );
  }
  meta.push(
    <Label key="priority" hue={board.priority === "high" ? "green" : "gray"}>
      {t(`boards.priority.${board.priority}`)}
    </Label>,
  );
  if (board.country !== undefined && board.country !== null) {
    meta.push(
      <Label key="country" hue="blue">
        {board.country}
      </Label>,
    );
  }
  if (salary.muted) {
    meta.push(
      <Label key="muted" hue="gray">
        {board.ranking.basis === "few" ? t("boards.salary.fewLabel") : t("boards.salary.noneLabel")}
      </Label>,
    );
  }
  if (board.flags.length > 0) {
    meta.push(
      <Label key="flags" hue="orange">
        {t("boards.flags", { n: String(board.flags.length) })}
      </Label>,
    );
  }
  const detail = rowDetail(board, i18n.language, t);
  return (
    <BoardCard
      title={board.name}
      subtitle={fit === null ? salary.text : `${salary.text} · ${fit}`}
      meta={
        <Stack gap="sm">
          <Cluster>{meta}</Cluster>
          {detail === "" ? null : <Text tone="secondary">{detail}</Text>}
        </Stack>
      }
      onOpen={onOpen}
      testId="boards-row"
    />
  );
}

/** What a row says under its labels: why it was declined, its last error, its last fetch. */
function rowDetail(board: BoardSummary, locale: string, t: TFunction<"sources">): string {
  if (board.status === "declined") {
    return board.decline_reason ?? "";
  }
  if (board.last_error !== undefined && board.last_error !== null) {
    return t("boards.lastError", { error: board.last_error });
  }
  const lastFetch = board.last_fetch_at ?? null;
  if (lastFetch === null) {
    return board.note ?? "";
  }
  return t("boards.lastFetch", {
    when: formatWhen(lastFetch, locale).text,
    jobs: String(board.last_fetch_jobs ?? 0),
    matched: String(board.totals_7d.matched),
  });
}

/** "Suggest a board": its site becomes a candidate, checked in the background. */
function SuggestBoard() {
  const { t } = useTranslation("sources");
  const suggest = useSuggestBoard();
  const [url, setUrl] = useState("");
  const submit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const value = url.trim();
    if (value === "") {
      return;
    }
    suggest.mutate(value, {
      onSuccess: () => {
        setUrl("");
      },
    });
  };
  const error = suggest.isError
    ? t(`boards.errors.${boardErrorOf(suggest.error).code}`)
    : undefined;
  return (
    <Tagged testId="boards-suggest-form">
      <form onSubmit={submit} noValidate>
        <Stack gap="sm">
          <Cluster>
            <TextField
              label={t("boards.suggest.label")}
              type="url"
              inputMode="url"
              autoComplete="off"
              placeholder="https://"
              value={url}
              error={error}
              onChange={(event) => {
                setUrl(event.target.value);
              }}
              data-testid="boards-suggest-url"
            />
            <Button
              type="submit"
              variant="secondary"
              icon="plus"
              disabled={suggest.isPending}
              data-testid="boards-suggest"
            >
              {t("boards.suggest.button")}
            </Button>
          </Cluster>
          {suggest.isSuccess ? (
            <Tagged testId="boards-suggest-done">
              <Text tone="secondary">
                {t("boards.suggest.done", { name: suggest.data.board.name })}
              </Text>
            </Tagged>
          ) : null}
        </Stack>
      </form>
    </Tagged>
  );
}
