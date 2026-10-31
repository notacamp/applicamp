import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Cluster,
  PickList,
  PickRow,
  Placeholder,
  SplitView,
  Stack,
  StatusText,
  Tagged,
  Text,
} from "~/lib/ui";

import { kindLabel, startedText, statusOf } from "../../api/labels";
import { PROBE, startErrorOf, useRuns, useStartCheck, type Run } from "../../api/runs";
import { RunDetail } from "../RunDetail/RunDetail";

export interface RunsPageProps {
  /** The run asked for (`?run=`); without one, the newest is shown beside the list. */
  selected: string | undefined;
  /** Show a run; `null` goes back to the list (phones). */
  onSelect: (id: string | null) => void;
}

/**
 * The Agents page: the runs on the left (kind, status, started), the chosen
 * one on the right, live; one column on a phone, with a way back. "Run a
 * check" starts the probe. Says so when this instance has no sandbox to run
 * agents in.
 */
export function RunsPage({ selected, onSelect }: RunsPageProps) {
  const { t } = useTranslation("runs");
  const runs = useRuns();
  const start = useStartCheck();

  if (runs.isPending) {
    return (
      <Tagged testId="runs-loading">
        <Text tone="secondary">{t("loading")}</Text>
      </Tagged>
    );
  }
  if (runs.isError) {
    return (
      <Tagged testId="runs-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t("loadFailed")}
          action={
            <Button
              variant="secondary"
              onClick={() => {
                void runs.refetch();
              }}
            >
              {t("retry")}
            </Button>
          }
        />
      </Tagged>
    );
  }

  const { runs: list, available, startable } = runs.data;
  const canStart = available && startable.includes(PROBE);
  const shown = selected ?? list[0]?.id;
  return (
    <Stack gap="lg">
      {available ? null : (
        <Tagged testId="runs-unavailable">
          <Banner tone="neutral" icon="lock" title={t("unavailableTitle")}>
            {t("unavailableBody")}
          </Banner>
        </Tagged>
      )}
      {canStart ? (
        <Cluster>
          <Button
            variant="primary"
            icon="check"
            disabled={start.isPending}
            data-testid="run-check"
            onClick={() => {
              start.mutate(undefined, {
                onSuccess: (run) => {
                  onSelect(run.id);
                },
              });
            }}
          >
            {start.isPending ? t("starting") : t("runCheck")}
          </Button>
        </Cluster>
      ) : null}
      {start.isError ? (
        <Tagged testId="run-check-error">
          <Banner
            tone="danger"
            icon="warning"
            role="alert"
            title={t(`errors.${startErrorOf(start.error)}`)}
          />
        </Tagged>
      ) : null}
      {list.length === 0 ? (
        <Placeholder testId="agents-empty" icon="hourglass" title={t("emptyTitle")}>
          {canStart ? `${t("emptyBody")} ${t("emptyCheck")}` : t("emptyBody")}
        </Placeholder>
      ) : (
        <SplitView
          testId="runs"
          listLabel={t("list")}
          detailLabel={t("run")}
          showing={selected === undefined ? "list" : "detail"}
          back={
            <Button
              variant="ghost"
              icon="chevron-left"
              data-testid="runs-back"
              onClick={() => {
                onSelect(null);
              }}
            >
              {t("back")}
            </Button>
          }
          list={
            <PickList label={t("list")} testId="runs-list">
              {list.map((run) => (
                <RunRow
                  key={run.id}
                  run={run}
                  selected={run.id === shown}
                  onSelect={() => {
                    onSelect(run.id);
                  }}
                />
              ))}
            </PickList>
          }
          detail={
            shown === undefined ? null : (
              <RunDetail key={shown} id={shown} initial={list.find((run) => run.id === shown)} />
            )
          }
        />
      )}
    </Stack>
  );
}

/** One run in the list: its kind, where it is and when it started. */
function RunRow({
  run,
  selected,
  onSelect,
}: {
  run: Run;
  selected: boolean;
  onSelect: () => void;
}) {
  const { t, i18n } = useTranslation("runs");
  const status = statusOf(run, t);
  return (
    <PickRow
      title={kindLabel(run.kind, t)}
      meta={
        <StatusText tone={status.tone}>
          {t("rowMeta", { status: status.text, when: startedText(run, i18n.language, t) })}
        </StatusText>
      }
      selected={selected}
      onSelect={onSelect}
      testId="runs-row"
      data={{ "data-id": run.id, "data-status": run.status, "data-outcome": run.outcome ?? "" }}
    />
  );
}
