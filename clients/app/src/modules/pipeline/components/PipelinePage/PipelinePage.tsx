import { useDeferredValue, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { useShortcut } from "~/lib/shortcuts";
import {
  Banner,
  Button,
  Cluster,
  Placeholder,
  SearchField,
  Select,
  Stack,
  Tagged,
  Text,
  useNarrowScreen,
  ViewSwitch,
} from "~/lib/ui";

import {
  STAGES,
  useOpportunities,
  type OpportunityStage,
  type OpportunitySummary,
} from "../../api/opportunities";
import { NewOpportunityDialog } from "../NewOpportunity/NewOpportunityDialog";
import { OpportunityPeek } from "../OpportunityPeek/OpportunityPeek";

import { OpportunityBoard, OpportunityCards, OpportunityTable } from "./OpportunityViews";

type View = "table" | "board";
type StageFilter = OpportunityStage | "all";

export interface PipelinePageProps {
  /** Only the new matches (Today's "N new matches"). */
  newOnly: boolean;
  onNewOnly: (newOnly: boolean) => void;
  /** The New dialog is asked for (⌘K's "New opportunity"). */
  adding: boolean;
  onAdding: (open: boolean) => void;
  /** Beside New: the Search now button (the sources module's). */
  search?: ReactNode;
  /** Under the toolbar: the search's progress and its last line. */
  progress?: ReactNode;
}

/**
 * The Pipeline: every opportunity in one database, as a table ("All
 * opportunities", cards on a phone) or a board by stage; filtered by stage
 * and words; each opening in the side peek. J and K move the cursor, Enter
 * opens it, Escape closes it.
 */
export function PipelinePage({
  newOnly,
  onNewOnly,
  adding,
  onAdding,
  search,
  progress,
}: PipelinePageProps) {
  const { t } = useTranslation("pipeline");
  const narrow = useNarrowScreen();
  const [view, setView] = useState<View>("table");
  const [stage, setStage] = useState<StageFilter>("all");
  const [query, setQuery] = useState("");
  const q = useDeferredValue(query);
  const [cursor, setCursor] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);

  const list = useOpportunities({
    stage: stage === "all" ? undefined : stage,
    q,
    new: newOnly,
  });
  const rows: readonly OpportunitySummary[] = list.data ?? [];
  // The board reads left to right, column by column; the table top to bottom.
  const order =
    view === "board"
      ? STAGES.flatMap((column) => rows.filter((row) => row.stage === column))
      : rows;

  const move = (by: number) => {
    if (order.length === 0) {
      return;
    }
    const at = order.findIndex((row) => row.id === cursor);
    const next = at === -1 ? (by > 0 ? 0 : order.length - 1) : at + by;
    const row = order[Math.min(order.length - 1, Math.max(0, next))];
    if (row !== undefined) {
      setCursor(row.id);
    }
  };
  const listKeys = { enabled: open === null && !adding && order.length > 0 };
  useShortcut(
    "list.next",
    () => {
      move(1);
    },
    listKeys,
  );
  useShortcut(
    "list.previous",
    () => {
      move(-1);
    },
    listKeys,
  );
  useShortcut(
    "list.open",
    () => {
      if (cursor !== null) {
        setOpen(cursor);
      }
    },
    { enabled: listKeys.enabled && cursor !== null },
  );

  const openRow = (row: OpportunitySummary) => {
    setCursor(row.id);
    setOpen(row.id);
  };
  const filtered = stage !== "all" || q.trim() !== "" || newOnly;

  let body: ReactNode;
  if (list.isPending) {
    body = (
      <Tagged testId="pipeline-loading">
        <Text tone="secondary">{t("loading")}</Text>
      </Tagged>
    );
  } else if (list.isError) {
    body = (
      <Tagged testId="pipeline-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void list.refetch();
              }}
            >
              {t("retry")}
            </Button>
          }
        />
      </Tagged>
    );
  } else if (rows.length === 0) {
    body = filtered ? (
      <Placeholder testId="pipeline-no-results" icon="search" title={t("empty.filteredTitle")}>
        {t("empty.filteredBody")}
      </Placeholder>
    ) : (
      <Placeholder testId="pipeline-empty" icon="tasks" title={t("empty.title")}>
        {t("empty.body")}
      </Placeholder>
    );
  } else if (view === "board") {
    body = <OpportunityBoard rows={rows} selected={cursor} onOpen={openRow} />;
  } else if (narrow) {
    body = <OpportunityCards rows={rows} selected={cursor} onOpen={openRow} />;
  } else {
    body = <OpportunityTable rows={rows} selected={cursor} onOpen={openRow} />;
  }

  return (
    <Stack>
      <Tagged testId="pipeline-toolbar">
        <Cluster>
          <ViewSwitch
            label={t("views.label")}
            options={[
              {
                value: "table",
                label: t("views.table"),
                icon: "bullets",
                testId: "pipeline-view-table",
              },
              {
                value: "board",
                label: t("views.board"),
                icon: "tasks",
                testId: "pipeline-view-board",
              },
            ]}
            value={view}
            onChange={setView}
          />
          <Button
            variant="secondary"
            icon="plus"
            shortcut="pipeline.add"
            data-testid="pipeline-new"
            onClick={() => {
              onAdding(true);
            }}
          >
            {t("new.button")}
          </Button>
          {search}
        </Cluster>
      </Tagged>
      <Cluster>
        <SearchField
          label={t("filter.search")}
          placeholder={t("filter.search")}
          value={query}
          onValueChange={setQuery}
          onClear={() => {
            setQuery("");
          }}
          variant="page"
          data-testid="pipeline-search"
        />
        <Select
          label={t("filter.stage")}
          hideLabel
          options={[
            { value: "all" as const, label: t("filter.allStages") },
            ...STAGES.map((value) => ({ value, label: t(`stages.${value}`) })),
          ]}
          value={stage}
          onChange={setStage}
          data-testid="pipeline-stage-filter"
        />
      </Cluster>
      {newOnly ? (
        <Tagged testId="pipeline-new-only">
          <Banner
            icon="check"
            title={t("filter.newOnly")}
            action={
              <Button
                variant="secondary"
                data-testid="pipeline-show-all"
                onClick={() => {
                  onNewOnly(false);
                }}
              >
                {t("filter.showAll")}
              </Button>
            }
          />
        </Tagged>
      ) : null}
      {progress}
      {body}
      <OpportunityPeek
        id={open}
        onClose={() => {
          setOpen(null);
        }}
      />
      <NewOpportunityDialog
        open={adding}
        onClose={() => {
          onAdding(false);
        }}
        onAdded={(opportunity) => {
          onAdding(false);
          setCursor(opportunity.id);
          setOpen(opportunity.id);
        }}
      />
    </Stack>
  );
}
