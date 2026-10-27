import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import {
  Board,
  BoardCard,
  BoardColumn,
  CardList,
  Cluster,
  DataTable,
  Label,
  MatchBar,
  Tagged,
  Text,
  type DataColumn,
} from "~/lib/ui";

import {
  formatShortDate,
  hiddenReasonText,
  sourceHue,
  sourceLabel,
  STAGE_HUES,
} from "../../api/labels";
import { STAGES, type OpportunitySummary } from "../../api/opportunities";

export interface OpportunityViewProps {
  rows: readonly OpportunitySummary[];
  /** The keyboard cursor. */
  selected: string | null;
  onOpen: (row: OpportunitySummary) => void;
  /** The hidden ones are listed: each says why it is hidden. */
  hidden?: boolean | undefined;
}

/** The role, or `untitled` for a link whose role is not typed yet. */
export function titleOf(row: Pick<OpportunitySummary, "title">, untitled: string): string {
  return row.title.trim() === "" ? untitled : row.title;
}

/** "Example Co · Remote - Europe". */
function companyLine(row: OpportunitySummary): string {
  const location = row.location ?? "";
  return location === "" ? row.company : `${row.company} · ${location}`;
}

/** Why `row` is hidden, as one line; nothing for one in view. */
function HiddenReasonLine({ row }: { row: OpportunitySummary }) {
  const { t } = useTranslation("pipeline");
  const reason = row.hidden_reason ?? null;
  if (reason === null) {
    return null;
  }
  return (
    <Tagged testId="pipeline-row-hidden-reason">
      <Text tone="secondary">{hiddenReasonText(reason, row.region_note, t)}</Text>
    </Tagged>
  );
}

function CardMeta({ row }: { row: OpportunitySummary }) {
  const { t } = useTranslation("pipeline");
  return (
    <Cluster>
      <MatchBar value={row.match} label={t("columns.match")} />
      <Label hue={STAGE_HUES[row.stage]}>{t(`stages.${row.stage}`)}</Label>
      <Label hue={sourceHue(row.source)}>{sourceLabel(row, t)}</Label>
      <HiddenReasonLine row={row} />
    </Cluster>
  );
}

/**
 * The table's columns: role, company, match, stage, source, next step (or why it is hidden),
 * updated.
 */
function tableColumns(
  t: TFunction<"pipeline">,
  language: string,
  hidden: boolean,
): DataColumn<OpportunitySummary>[] {
  const next: DataColumn<OpportunitySummary> = hidden
    ? {
        id: "hidden",
        header: t("columns.hiddenReason"),
        cell: (row) => <HiddenReasonLine row={row} />,
      }
    : { id: "next", header: t("columns.nextStep"), cell: (row) => row.next_step ?? "" };
  return [
    { id: "role", header: t("columns.role"), cell: (row) => titleOf(row, t("untitled")) },
    { id: "company", header: t("columns.company"), cell: (row) => row.company },
    {
      id: "match",
      header: t("columns.match"),
      nowrap: true,
      cell: (row) => <MatchBar value={row.match} label={t("columns.match")} />,
    },
    {
      id: "stage",
      header: t("columns.stage"),
      nowrap: true,
      cell: (row) => <Label hue={STAGE_HUES[row.stage]}>{t(`stages.${row.stage}`)}</Label>,
    },
    {
      id: "source",
      header: t("columns.source"),
      nowrap: true,
      cell: (row) => <Label hue={sourceHue(row.source)}>{sourceLabel(row, t)}</Label>,
    },
    next,
    {
      id: "updated",
      header: t("columns.updated"),
      nowrap: true,
      cell: (row) => formatShortDate(row.updated_at, language),
    },
  ];
}

/** "All opportunities" as the prototype's table: role, company, match, stage, source, next step, updated. */
export function OpportunityTable({ rows, selected, onOpen, hidden }: OpportunityViewProps) {
  const { t, i18n } = useTranslation("pipeline");
  const columns = tableColumns(t, i18n.language, hidden === true);
  return (
    <DataTable
      label={t("views.table")}
      columns={columns}
      rows={rows}
      rowKey={(row) => row.id}
      selected={selected}
      onOpen={onOpen}
      rowTestId="pipeline-row"
      openTestId="pipeline-row-open"
      testId="pipeline-table"
    />
  );
}

/** "All opportunities" on a phone: one card per opportunity. */
export function OpportunityCards({ rows, selected, onOpen }: OpportunityViewProps) {
  const { t } = useTranslation("pipeline");
  return (
    <CardList label={t("views.table")} testId="pipeline-cards">
      {rows.map((row) => (
        <BoardCard
          key={row.id}
          title={titleOf(row, t("untitled"))}
          subtitle={companyLine(row)}
          meta={<CardMeta row={row} />}
          selected={row.id === selected}
          onOpen={() => {
            onOpen(row);
          }}
          testId="pipeline-row"
        />
      ))}
    </CardList>
  );
}

/** "By stage": a column per stage, its cards in it. Scrolls sideways inside itself. */
export function OpportunityBoard({ rows, selected, onOpen }: OpportunityViewProps) {
  const { t } = useTranslation("pipeline");
  return (
    <Board label={t("views.board")} testId="pipeline-board">
      {STAGES.map((stage) => {
        const cards = rows.filter((row) => row.stage === stage);
        return (
          <BoardColumn
            key={stage}
            title={<Label hue={STAGE_HUES[stage]}>{t(`stages.${stage}`)}</Label>}
            count={cards.length}
            empty={t("board.empty")}
            testId={`pipeline-column-${stage}`}
          >
            {cards.map((row) => (
              <BoardCard
                key={row.id}
                title={titleOf(row, t("untitled"))}
                subtitle={companyLine(row)}
                meta={
                  <Cluster>
                    <MatchBar value={row.match} label={t("columns.match")} />
                    <Label hue={sourceHue(row.source)}>{sourceLabel(row, t)}</Label>
                  </Cluster>
                }
                selected={row.id === selected}
                onOpen={() => {
                  onOpen(row);
                }}
                testId="pipeline-card"
              />
            ))}
          </BoardColumn>
        );
      })}
    </Board>
  );
}
