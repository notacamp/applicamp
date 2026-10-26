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
  type DataColumn,
} from "~/lib/ui";

import { formatShortDate, SOURCE_HUES, STAGE_HUES } from "../../api/labels";
import { STAGES, type OpportunitySummary } from "../../api/opportunities";

export interface OpportunityViewProps {
  rows: readonly OpportunitySummary[];
  /** The keyboard cursor. */
  selected: string | null;
  onOpen: (row: OpportunitySummary) => void;
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

function CardMeta({ row }: { row: OpportunitySummary }) {
  const { t } = useTranslation("pipeline");
  return (
    <Cluster>
      <MatchBar value={row.match} label={t("columns.match")} />
      <Label hue={STAGE_HUES[row.stage]}>{t(`stages.${row.stage}`)}</Label>
      <Label hue={SOURCE_HUES[row.source]}>{t(`sources.${row.source}`)}</Label>
    </Cluster>
  );
}

/** "All opportunities" as the prototype's table: role, company, match, stage, source, next step, updated. */
export function OpportunityTable({ rows, selected, onOpen }: OpportunityViewProps) {
  const { t, i18n } = useTranslation("pipeline");
  const columns: DataColumn<OpportunitySummary>[] = [
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
      cell: (row) => <Label hue={SOURCE_HUES[row.source]}>{t(`sources.${row.source}`)}</Label>,
    },
    { id: "next", header: t("columns.nextStep"), cell: (row) => row.next_step ?? "" },
    {
      id: "updated",
      header: t("columns.updated"),
      nowrap: true,
      cell: (row) => formatShortDate(row.updated_at, i18n.language),
    },
  ];
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
                    <Label hue={SOURCE_HUES[row.source]}>{t(`sources.${row.source}`)}</Label>
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
