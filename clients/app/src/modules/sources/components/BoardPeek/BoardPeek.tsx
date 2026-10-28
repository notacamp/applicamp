import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Bullets,
  Button,
  ButtonLink,
  Cluster,
  Disclosure,
  Label,
  Property,
  PropertyList,
  Section,
  SidePeek,
  Stack,
  Tagged,
  Text,
  TextArea,
  TextField,
} from "~/lib/ui";

import {
  blockerCode,
  boardErrorOf,
  proposalLines,
  salaryLine,
  totalsLine,
  useBoard,
  useBoardAction,
  type BoardAction,
  type BoardDetail,
  type BoardKey,
  type BoardProposal,
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
 * the proposed attribution), and, for the owner, why it can't be approved
 * yet, Approve, Decline (with a reason), Pause, Check again, its API key, the
 * config its check proposes, and the config as JSON under "Advanced". Every
 * action's error shows beside its button, with what the server said.
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
        {board.skills_fit === undefined || board.skills_fit === null ? null : (
          <Property label={t("board.fit")} testId="board-fit">
            {t("board.fitValue", {
              share: String(Math.round(board.skills_fit.share * 100)),
              jobs: String(board.skills_fit.jobs),
            })}
          </Property>
        )}
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

/** An action that failed, beside its button: what it means, and the server's own words. */
function ActionError({ testId, error }: { testId: string; error: unknown }) {
  const { t } = useTranslation("sources");
  const { code, detail } = boardErrorOf(error);
  return (
    <Tagged testId={testId}>
      <Banner tone="danger" icon="warning" role="alert" title={t(`boards.errors.${code}`)}>
        {detail}
      </Banner>
    </Tagged>
  );
}

/** Why the board can't be approved yet, each reason in the app's words with what exactly. */
function Blockers({ detail }: { detail: BoardDetail }) {
  const { t } = useTranslation("sources");
  return (
    <Tagged testId="board-blocked">
      <Banner icon="warning" title={t("board.blocked.title")}>
        <Stack gap="sm">
          {detail.blockers.map((blocker, index) => {
            const code = blockerCode(blocker);
            const text = t(`board.blocked.${code}`, {
              detail: blocker.detail,
              board: detail.board.name,
              how: detail.key?.how ?? blocker.detail,
            });
            const source = blocker.source ?? null;
            return (
              <Tagged key={`${blocker.code}-${String(index)}`} testId={`board-blocker-${code}`}>
                <Stack gap="sm">
                  <Text>{text}</Text>
                  {source === null ? null : (
                    <Cluster>
                      <ButtonLink
                        variant="ghost"
                        icon="forward"
                        href={source}
                        target="_blank"
                        rel="noopener noreferrer"
                        data-testid="board-blocker-source"
                      >
                        {code === "needs_key" ? t("board.key.get") : t("board.blocked.source")}
                      </ButtonLink>
                    </Cluster>
                  )}
                </Stack>
              </Tagged>
            );
          })}
        </Stack>
      </Banner>
    </Tagged>
  );
}

/** The owner's decisions on a board. */
function Manage({ detail }: { detail: BoardDetail }) {
  const { t } = useTranslation("sources");
  const { board } = detail;
  const action = useBoardAction(board.id);
  const [reason, setReason] = useState("");
  const waiting = board.status === "candidate" || board.status === "paused";
  /** The error of `kind`, when it is the action that failed last. */
  const failed = (...kinds: BoardAction["kind"][]) =>
    action.isError && kinds.includes(action.variables.kind);
  const proposal = detail.report?.proposal ?? null;
  return (
    <Section title={t("board.manage.title")} testId="board-manage">
      <Stack>
        {waiting && detail.blockers.length > 0 ? <Blockers detail={detail} /> : null}
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
        {failed("approve") ? (
          <ActionError testId="board-approve-error" error={action.error} />
        ) : null}
        {failed("pause") ? <ActionError testId="board-pause-error" error={action.error} /> : null}
        {failed("check") ? <ActionError testId="board-check-error" error={action.error} /> : null}
        {action.isSuccess && action.variables.kind === "check" ? (
          <Tagged testId="board-check-started">
            <Text tone="secondary">{t("board.manage.checkStarted")}</Text>
          </Tagged>
        ) : null}
        {detail.key === undefined || detail.key === null ? null : (
          <KeyForm
            key={detail.key.set_at ?? "none"}
            detail={detail}
            keySpec={detail.key}
            pending={action.isPending}
            onSave={(values) => {
              action.mutate({ kind: "key", values });
            }}
            onForget={() => {
              action.mutate({ kind: "forgetKey" });
            }}
            error={failed("key", "forgetKey") ? action.error : null}
          />
        )}
        {proposal === null ? null : (
          <ProposalView
            proposal={proposal}
            pending={action.isPending}
            onUse={() => {
              action.mutate({ kind: "proposal", proposal });
            }}
            error={failed("proposal") ? action.error : null}
          />
        )}
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
            {failed("decline") ? (
              <ActionError testId="board-decline-error" error={action.error} />
            ) : null}
          </Stack>
        )}
        <Disclosure
          summary={<span data-testid="board-config-advanced">{t("board.config.advanced")}</span>}
        >
          <ConfigEditor
            key={JSON.stringify(detail.config)}
            config={detail.config}
            pending={action.isPending}
            onSave={(config) => {
              action.mutate({ kind: "config", config });
            }}
            error={failed("config") ? action.error : null}
          />
        </Disclosure>
      </Stack>
    </Section>
  );
}

/** The key a board's API needs: how to get one, whether one is set, and a field per parameter. */
function KeyForm({
  detail,
  keySpec,
  pending,
  onSave,
  onForget,
  error,
}: {
  detail: BoardDetail;
  keySpec: BoardKey;
  pending: boolean;
  onSave: (values: Record<string, string>) => void;
  onForget: () => void;
  error: unknown;
}) {
  const { t, i18n } = useTranslation("sources");
  const [values, setValues] = useState<Record<string, string>>({});
  const filled = keySpec.params.every((param) => (values[param] ?? "").trim() !== "");
  const setAt = keySpec.set_at ?? null;
  let state = t("board.key.none");
  if (keySpec.set) {
    state =
      setAt === null
        ? t("board.key.set")
        : t("board.key.setOn", { when: formatWhen(setAt, i18n.language).text });
  }
  return (
    <Section
      title={t("board.key.title")}
      description={t("board.key.how", { board: detail.board.name, how: keySpec.how })}
      testId="board-key"
    >
      <Stack gap="sm">
        <Tagged testId="board-key-state">
          <Text tone={keySpec.set ? undefined : "secondary"}>{state}</Text>
        </Tagged>
        {keySpec.params.map((param) => (
          <TextField
            key={param}
            label={t("board.key.field", { param })}
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={values[param] ?? ""}
            onChange={(event) => {
              setValues((current) => ({ ...current, [param]: event.target.value }));
            }}
            data-testid={`board-key-${param}`}
          />
        ))}
        <Cluster>
          <Button
            variant="secondary"
            icon="check"
            disabled={!filled || pending}
            data-testid="board-key-save"
            onClick={() => {
              onSave(
                Object.fromEntries(
                  keySpec.params.map((param) => [param, (values[param] ?? "").trim()]),
                ),
              );
            }}
          >
            {keySpec.set ? t("board.key.replace") : t("board.key.save")}
          </Button>
          <ButtonLink
            variant="ghost"
            icon="forward"
            href={keySpec.url}
            target="_blank"
            rel="noopener noreferrer"
            data-testid="board-key-get"
          >
            {t("board.key.get")}
          </ButtonLink>
          {keySpec.set ? (
            <Button
              variant="ghost"
              icon="close"
              disabled={pending}
              data-testid="board-key-forget"
              onClick={onForget}
            >
              {t("board.key.forget")}
            </Button>
          ) : null}
        </Cluster>
        {error === null ? null : <ActionError testId="board-key-error" error={error} />}
      </Stack>
    </Section>
  );
}

/** A config the check found, as fields and where each is read from; "Use this config". */
function ProposalView({
  proposal,
  pending,
  onUse,
  error,
}: {
  proposal: BoardProposal;
  pending: boolean;
  onUse: () => void;
  error: unknown;
}) {
  const { t } = useTranslation("sources");
  const lines = proposalLines(proposal);
  return (
    <Section
      title={t("board.proposal.title")}
      description={t("board.proposal.description", {
        kind: t(`board.proposal.kind.${proposal.kind}`),
      })}
      testId="board-proposal"
    >
      <Stack gap="sm">
        <PropertyList testId="board-proposal-fields">
          {lines.address === undefined ? null : (
            <Property label={t("board.proposal.address")}>{lines.address}</Property>
          )}
          {lines.items === undefined ? null : (
            <Property label={t("board.proposal.items")}>{lines.items}</Property>
          )}
          {lines.fields.map(({ field, source }) => (
            <Property key={field} label={t(`board.proposal.fields.${field}`)}>
              {source}
            </Property>
          ))}
        </PropertyList>
        <Cluster>
          <Button
            variant="primary"
            icon="check"
            disabled={pending}
            data-testid="board-proposal-use"
            onClick={onUse}
          >
            {t("board.proposal.use")}
          </Button>
        </Cluster>
        {error === null ? null : <ActionError testId="board-proposal-error" error={error} />}
      </Stack>
    </Section>
  );
}

/** The config as JSON, for the owner who wants to write it by hand; its errors beside Save. */
function ConfigEditor({
  config: initial,
  pending,
  onSave,
  error,
}: {
  config: BoardDetail["config"];
  pending: boolean;
  onSave: (config: Record<string, unknown>) => void;
  error: unknown;
}) {
  const { t } = useTranslation("sources");
  const [config, setConfig] = useState(() => JSON.stringify(initial, null, 2));
  const [local, setLocal] = useState("");
  const save = () => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(config);
    } catch {
      setLocal(t("board.config.notJson"));
      return;
    }
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      setLocal(t("board.config.notObject"));
      return;
    }
    setLocal("");
    onSave(parsed as Record<string, unknown>);
  };
  return (
    <Stack gap="sm">
      <TextArea
        label={t("board.config.label")}
        hint={t("board.config.hint")}
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
          disabled={pending}
          data-testid="board-config-save"
          onClick={save}
        >
          {t("board.config.save")}
        </Button>
      </Cluster>
      {local === "" ? null : (
        <Tagged testId="board-config-error">
          <Banner tone="danger" icon="warning" role="alert" title={local} />
        </Tagged>
      )}
      {local === "" && error !== null ? (
        <ActionError testId="board-config-error" error={error} />
      ) : null}
    </Stack>
  );
}
