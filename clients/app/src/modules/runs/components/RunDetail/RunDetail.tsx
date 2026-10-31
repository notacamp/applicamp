import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Cluster,
  EventLine,
  EventLog,
  Heading,
  Label,
  Property,
  PropertyList,
  Section,
  Stack,
  StatusText,
  Tagged,
  Text,
  ViewSwitch,
} from "~/lib/ui";

import { formatClock, formatCost, formatCount, formatDuration } from "../../api/format";
import { badgeOf, failureOf, kindLabel, statusOf } from "../../api/labels";
import { isLive, useRun, useStopRun, type Run } from "../../api/runs";
import { useRunEvents, type Layer, type RunEvent, type StreamState } from "../../api/stream";

export interface RunDetailProps {
  /** The run. `key` the component by it: its live view is that run's. */
  id: string;
  /** What the list already says about it, shown until it answers itself. */
  initial?: Run | undefined;
}

/**
 * One run: where it is (Stop while it goes), what its agent does and its
 * sandbox's network log, live, and its facts (sandbox, image, policy,
 * model, tokens, cost, duration, end state).
 */
export function RunDetail({ id, initial }: RunDetailProps) {
  const { t } = useTranslation("runs");
  const run = useRun(id, initial);
  const feed = useRunEvents(id);
  const [layer, setLayer] = useState<Layer>("agent");

  if (run.data === undefined) {
    return run.isError ? (
      <Tagged testId="run-load-error">
        <Banner tone="danger" icon="warning" title={t("loadFailed")} />
      </Tagged>
    ) : (
      <Tagged testId="run-loading">
        <Text tone="secondary">{t("loading")}</Text>
      </Tagged>
    );
  }
  const current = run.data;
  return (
    <Stack gap="lg" testId="run-detail">
      <RunHead run={current} />
      <Stack>
        <ViewSwitch
          label={t("tabs.label")}
          options={[
            { value: "agent" as const, label: t("tabs.agent"), testId: "run-tab-agent" },
            { value: "sandbox" as const, label: t("tabs.sandbox"), testId: "run-tab-sandbox" },
          ]}
          value={layer}
          onChange={setLayer}
        />
        {feed.state === "lost" ? (
          <Tagged testId="run-live-lost">
            <Banner
              tone="neutral"
              icon="offline"
              title={t("live.lost")}
              action={
                <Button variant="secondary" data-testid="run-live-retry" onClick={feed.retry}>
                  {t("live.reconnect")}
                </Button>
              }
            />
          </Tagged>
        ) : null}
        <Lines
          layer={layer}
          events={feed.events.filter((event) => event.layer === layer)}
          state={feed.state}
          over={!isLive(current)}
        />
      </Stack>
      <Facts run={current} />
    </Stack>
  );
}

/** The run's kind and where it is, with Stop while it goes. */
function RunHead({ run }: { run: Run }) {
  const { t } = useTranslation("runs");
  const stop = useStopRun();
  const status = statusOf(run, t);
  const failure = failureOf(run, t);
  return (
    <Stack gap="sm">
      <Cluster>
        <Heading level={2} size="title">
          {kindLabel(run.kind, t)}
        </Heading>
        <Tagged
          testId="run-status"
          data={{ "data-status": run.status, "data-outcome": run.outcome ?? "" }}
        >
          <StatusText tone={status.tone}>{status.text}</StatusText>
        </Tagged>
        {isLive(run) && !run.stopping ? (
          <Button
            variant="secondary"
            icon="close"
            disabled={stop.isPending}
            data-testid="run-stop"
            onClick={() => {
              stop.mutate(run.id);
            }}
          >
            {t("stop")}
          </Button>
        ) : null}
      </Cluster>
      {failure === null ? null : (
        <Tagged testId="run-failure">
          <Text tone="secondary">{failure}</Text>
        </Tagged>
      )}
      {stop.isError ? (
        <Tagged testId="run-stop-error">
          <Banner tone="danger" icon="warning" role="alert" title={t("stopFailed")} />
        </Tagged>
      ) : null}
    </Stack>
  );
}

/** One layer's lines, or what it says while there are none. */
function Lines({
  layer,
  events,
  state,
  over,
}: {
  layer: Layer;
  events: RunEvent[];
  state: StreamState;
  /** The run has ended: no more lines are coming. */
  over: boolean;
}) {
  const { t, i18n } = useTranslation("runs");
  if (events.length === 0) {
    let text: string;
    if (state === "connecting") {
      text = t("live.connecting");
    } else if (over || state === "ended") {
      text = t(`noEventsEnded.${layer}`);
    } else {
      text = t(`noEvents.${layer}`);
    }
    return (
      <Tagged testId="run-log-empty">
        <Text tone="secondary">{text}</Text>
      </Tagged>
    );
  }
  return (
    <EventLog label={t(`tabs.${layer}`)} testId={`run-log-${layer}`}>
      {events.map((event) => {
        const badge = badgeOf(event, t);
        return (
          <EventLine
            key={event.seq}
            at={event.at}
            atText={formatClock(event.at, i18n.language)}
            badge={badge === null ? undefined : <Label hue={badge.hue}>{badge.text}</Label>}
            mono={layer === "sandbox"}
            testId="run-event"
            data={{ "data-level": event.level, "data-layer": event.layer }}
          >
            {event.message}
          </EventLine>
        );
      })}
    </EventLog>
  );
}

/** What the run ran in and what it used. */
function Facts({ run }: { run: Run }) {
  const { t, i18n } = useTranslation("runs");
  const locale = i18n.language;
  const notYet = t("facts.notYet");
  const or = (value: string | null | undefined) =>
    value === null || value === undefined || value === "" ? notYet : value;
  let model: string;
  if (run.model !== null && run.model !== undefined && run.model !== "") {
    model = run.model;
  } else {
    model = isLive(run) ? notYet : t("facts.noModel");
  }
  return (
    <Section title={t("facts.title")} testId="run-facts">
      <PropertyList>
        <Property label={t("facts.end")} testId="run-fact-end">
          {statusOf(run, t).text}
        </Property>
        <Property label={t("facts.duration")} testId="run-fact-duration">
          {run.duration_ms === null || run.duration_ms === undefined
            ? notYet
            : formatDuration(run.duration_ms, locale)}
        </Property>
        <Property label={t("facts.sandbox")} testId="run-fact-sandbox">
          {or(run.sandbox)}
        </Property>
        <Property label={t("facts.image")} testId="run-fact-image">
          {or(run.image)}
        </Property>
        <Property label={t("facts.policy")} testId="run-fact-policy">
          {or(run.policy)}
        </Property>
        <Property label={t("facts.model")} testId="run-fact-model">
          {model}
        </Property>
        <Property label={t("facts.tokens")} testId="run-fact-tokens">
          {t("facts.tokensValue", {
            in: formatCount(run.tokens_in, locale),
            out: formatCount(run.tokens_out, locale),
          })}
        </Property>
        <Property label={t("facts.cost")} testId="run-fact-cost">
          {formatCost(run.cost_eur, locale)}
        </Property>
      </PropertyList>
    </Section>
  );
}
