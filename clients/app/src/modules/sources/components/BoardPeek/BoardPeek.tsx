import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Bullets,
  Button,
  ButtonLink,
  Cluster,
  Label,
  Property,
  PropertyList,
  Section,
  SidePeek,
  Stack,
  Tagged,
  Text,
  TextArea,
} from "~/lib/ui";

import {
  boardErrorOf,
  salaryLine,
  totalsLine,
  useBoard,
  useBoardAction,
  type BoardDetail,
} from "../../api/boards";
import { formatWhen } from "../../api/format";

export interface BoardPeekProps {
  /** The board open; none closes the peek. */
  id: string | null;
  onClose: () => void;
}

/**
 * One board beside the list: its salary ranking, its terms, what its
 * automatic check found (robots rules, the terms' sentences, sample jobs,
 * the proposed attribution), and, for the owner, Approve, Decline (with a
 * reason), Pause, Check again and Edit config.
 */
export function BoardPeek({ id, onClose }: BoardPeekProps) {
  const { t } = useTranslation("sources");
  const board = useBoard(id);
  const data = id === null ? undefined : board.data;
  return (
    <SidePeek
      open={id !== null}
      label={data?.board.name ?? t("board.eyebrow")}
      eyebrow={t("board.eyebrow")}
      closeLabel={t("board.close")}
      onClose={onClose}
      testId="board-peek"
      closeTestId="board-peek-close"
    >
      {board.isError ? (
        <Tagged testId="board-peek-error">
          <Banner tone="danger" icon="warning" title={t("boards.loadFailed")} />
        </Tagged>
      ) : null}
      {data === undefined && !board.isError ? (
        <Text tone="secondary">{t("boards.loading")}</Text>
      ) : null}
      {data === undefined ? null : <PeekBody key={data.board.id} detail={data} />}
    </SidePeek>
  );
}

function percent(share: number | null | undefined): string | null {
  return share === undefined || share === null ? null : `${String(Math.round(share * 100))}%`;
}

function PeekBody({ detail }: { detail: BoardDetail }) {
  const { t, i18n } = useTranslation("sources");
  const language = i18n.language;
  const { board, report, terms } = detail;
  const salary = salaryLine(board.ranking, language, t);
  const robots = report?.robots;
  let robotsText = t("board.robots.unchecked");
  if (robots !== undefined && report?.checked_at !== undefined && report.checked_at !== null) {
    if (!robots.found) {
      robotsText = t("board.robots.none");
    } else if (robots.endpoint_allowed === false) {
      robotsText = t("board.robots.disallowed", { rule: robots.deciding_rule ?? "" });
    } else {
      robotsText = t("board.robots.allowed");
    }
  }
  const asks = [
    terms.link_back ? t("board.terms.linkBack") : null,
    terms.apply_unchanged ? t("board.terms.applyUnchanged") : null,
    terms.no_redistribution ? t("board.terms.noRedistribution") : null,
    terms.drop_expired ? t("board.terms.dropExpired") : null,
    terms.no_logo ? t("board.terms.noLogo") : null,
    terms.max_fetches_per_day === undefined || terms.max_fetches_per_day === null
      ? null
      : t("board.terms.maxFetches", { n: String(terms.max_fetches_per_day) }),
  ].filter((ask): ask is string => ask !== null);
  const sample = report?.sample ?? [];
  const sentences = report?.terms_sentences ?? [];
  const shares = [
    percent(report?.remote_share) === null
      ? null
      : t("board.report.remote", { share: percent(report?.remote_share) ?? "" }),
    percent(report?.worldwide_share) === null
      ? null
      : t("board.report.worldwide", { share: percent(report?.worldwide_share) ?? "" }),
  ].filter((part): part is string => part !== null);
  return (
    <Stack>
      <PropertyList testId="board-properties">
        <Property label={t("board.status")}>
          <Label hue={board.status === "approved" ? "green" : "gray"}>
            {t(`boards.status.${board.status}`)}
          </Label>
        </Property>
        <Property label={t("board.salary")} testId="board-salary">
          {salary.muted ? <Text tone="secondary">{salary.text}</Text> : <Text>{salary.text}</Text>}
        </Property>
        <Property label={t("board.priority")}>{t(`boards.priority.${board.priority}`)}</Property>
        <Property label={t("board.credit")} testId="board-attribution">
          {board.attribution}
        </Property>
        {detail.endpoint === undefined || detail.endpoint === null ? null : (
          <Property label={t("board.endpoint")} testId="board-endpoint">
            {detail.endpoint}
          </Property>
        )}
        {board.last_fetch_at === undefined || board.last_fetch_at === null ? null : (
          <Property label={t("board.lastFetch")}>
            {t("board.lastFetchValue", {
              when: formatWhen(board.last_fetch_at, language).text,
              jobs: String(board.last_fetch_jobs ?? 0),
            })}
          </Property>
        )}
        {board.last_error === undefined || board.last_error === null ? null : (
          <Property label={t("board.lastError")}>{board.last_error}</Property>
        )}
        <Property label={t("board.totals")} testId="board-totals">
          <Stack gap="sm">
            <Text>{totalsLine(board.totals_7d, language, t("board.days7"), t)}</Text>
            <Text>{totalsLine(board.totals_30d, language, t("board.days30"), t)}</Text>
          </Stack>
        </Property>
      </PropertyList>
      {board.flags.length === 0 ? null : (
        <Tagged testId="board-flags">
          <Banner icon="warning" title={t("board.flagsTitle")}>
            {board.flags.join(" · ")}
          </Banner>
        </Tagged>
      )}
      {board.note === undefined || board.note === null ? null : (
        <Tagged testId="board-report-notes">
          <Text tone="secondary">{board.note}</Text>
        </Tagged>
      )}
      <Section title={t("board.report.title")} testId="board-report">
        <Stack gap="sm">
          <Tagged testId="board-robots">
            <Text>{robotsText}</Text>
          </Tagged>
          {report?.found_url === undefined || report.found_url === null ? null : (
            <Text tone="secondary">{t("board.report.found", { url: report.found_url })}</Text>
          )}
          {shares.length === 0 ? null : <Text tone="secondary">{shares.join(" · ")}</Text>}
          {report?.language === undefined || report.language === null ? null : (
            <Text tone="secondary">
              {t("board.report.language", { language: report.language })}
            </Text>
          )}
        </Stack>
      </Section>
      {sentences.length === 0 ? null : (
        <Section
          title={t("board.report.sentences")}
          description={t("board.report.sentencesHint")}
          testId="board-terms-sentences"
        >
          <Bullets items={sentences} />
        </Section>
      )}
      {sample.length === 0 ? null : (
        <Section title={t("board.report.sample")} testId="board-sample">
          <Bullets
            items={sample.map((job) =>
              [job.title, job.company, job.location ?? ""]
                .filter((part) => part !== "")
                .join(" · "),
            )}
          />
        </Section>
      )}
      {asks.length === 0 ? null : (
        <Section title={t("board.terms.title")} testId="board-terms">
          <Bullets items={asks} />
          {(terms.notes ?? "") === "" ? null : <Text tone="secondary">{terms.notes}</Text>}
        </Section>
      )}
      <Cluster>
        <ButtonLink
          variant="ghost"
          icon="forward"
          href={board.homepage}
          target="_blank"
          rel="noopener noreferrer"
          data-testid="board-homepage"
        >
          {t("board.homepage")}
        </ButtonLink>
      </Cluster>
      {detail.can_manage ? <Manage detail={detail} /> : null}
    </Stack>
  );
}

/** Why a board cannot be approved yet. */
const BLOCKS = ["check_needed", "robots_disallowed", "config_invalid"] as const;

/** The owner's decisions on a board. */
function Manage({ detail }: { detail: BoardDetail }) {
  const { t } = useTranslation("sources");
  const { board } = detail;
  const action = useBoardAction(board.id);
  const [reason, setReason] = useState("");
  const [config, setConfig] = useState(() => JSON.stringify(detail.config, null, 2));
  const [configError, setConfigError] = useState<string | undefined>(undefined);
  const waiting = board.status === "candidate" || board.status === "paused";
  const saveConfig = () => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(config);
    } catch {
      setConfigError(t("board.config.notJson"));
      return;
    }
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      setConfigError(t("board.config.notObject"));
      return;
    }
    setConfigError(undefined);
    action.mutate({ kind: "config", config: parsed as Record<string, unknown> });
  };
  const blocked = BLOCKS.find((block) => block === detail.blocked);
  return (
    <Section title={t("board.manage.title")} testId="board-manage">
      <Stack>
        {action.isError ? (
          <Tagged testId="board-action-error">
            <Banner
              tone="danger"
              icon="warning"
              role="alert"
              title={t(`boards.errors.${boardErrorOf(action.error)}`)}
            />
          </Tagged>
        ) : null}
        {waiting && blocked !== undefined ? (
          <Tagged testId="board-blocked">
            <Text tone="secondary">{t(`board.blocked.${blocked}`)}</Text>
          </Tagged>
        ) : null}
        <Cluster>
          {waiting ? (
            <Button
              variant="primary"
              icon="check"
              disabled={!detail.approvable || action.isPending}
              data-testid="board-approve"
              onClick={() => {
                action.mutate({ kind: "approve" });
              }}
            >
              {t("board.manage.approve")}
            </Button>
          ) : null}
          {board.status === "approved" ? (
            <Button
              variant="secondary"
              disabled={action.isPending}
              data-testid="board-pause"
              onClick={() => {
                action.mutate({ kind: "pause" });
              }}
            >
              {t("board.manage.pause")}
            </Button>
          ) : null}
          <Button
            variant="secondary"
            icon="search"
            disabled={action.isPending}
            data-testid="board-check"
            onClick={() => {
              action.mutate({ kind: "check" });
            }}
          >
            {t("board.manage.check")}
          </Button>
        </Cluster>
        {board.status === "declined" ? null : (
          <Stack gap="sm">
            <TextArea
              label={t("board.manage.reason")}
              rows={2}
              maxLength={500}
              value={reason}
              onChange={(event) => {
                setReason(event.target.value);
              }}
              data-testid="board-decline-reason"
            />
            <Cluster>
              <Button
                variant="ghost"
                icon="close"
                disabled={reason.trim() === "" || action.isPending}
                data-testid="board-decline"
                onClick={() => {
                  action.mutate({ kind: "decline", reason: reason.trim() });
                }}
              >
                {t("board.manage.decline")}
              </Button>
            </Cluster>
          </Stack>
        )}
        <Stack gap="sm">
          <TextArea
            label={t("board.config.label")}
            hint={t("board.config.hint")}
            error={configError}
            rows={12}
            spellCheck={false}
            value={config}
            onChange={(event) => {
              setConfig(event.target.value);
            }}
            data-testid="board-config"
          />
          <Cluster>
            <Button
              variant="secondary"
              disabled={action.isPending}
              data-testid="board-config-save"
              onClick={saveConfig}
            >
              {t("board.config.save")}
            </Button>
          </Cluster>
        </Stack>
      </Stack>
    </Section>
  );
}
