import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  ButtonLink,
  Cluster,
  CopyField,
  Heading,
  InlineConfirm,
  Property,
  PropertyList,
  Section,
  SidePeek,
  Skeleton,
  Stack,
  StatusMark,
  Steps,
  Tagged,
  Text,
  TextField,
} from "~/lib/ui";

import {
  howRead,
  useAnswerRequest,
  useMarkAsked,
  usePermissionEmail,
  useReopen,
} from "../../api/admin";
import type { AdminRow, RowStatus } from "../../api/admin-rows";
import {
  boardErrorOf,
  type BoardErrorCode,
  type BoardStatus,
  useBoard,
  useBoardAction,
  type BoardAction,
  type BoardDetail,
} from "../../api/boards";
import { formatWhen, shortDate } from "../../api/format";
import { BoardPeek } from "../BoardPeek/BoardPeek";
import { StageLabel } from "./StageLabel";
import { WhyLine } from "./WhyLine";

/** What a decline from this page says, kept with the board. */
const DECLINED_HERE = "Declined from the Job sites page.";

export interface AdminPeekProps {
  /** The board open; none closes the panel. */
  row: AdminRow | undefined;
  onClose: () => void;
  /** The owner declined the board from here. */
  onDeclined: (id: string) => void;
}

/** The row, brought up to date with what the board's own panel says it is now. */
function reconcile(row: AdminRow, detail: BoardDetail | undefined): AdminRow {
  const status: BoardStatus | undefined = detail?.board.status;
  if (detail === undefined || status === undefined || status === row.board.status) {
    return row;
  }
  let next: RowStatus = row.status;
  switch (status) {
    case "approved": {
      next = "reading";
      break;
    }
    case "paused": {
      next = "paused";
      break;
    }
    case "declined": {
      next = "notAllowed";
      break;
    }
    case "candidate": {
      if (row.status === "reading" || row.status === "paused" || row.status === "notAllowed") {
        next = "waiting";
      }
      break;
    }
  }
  return {
    ...row,
    status: next,
    need: next === "waiting" ? (row.need ?? "approve") : undefined,
    board: { ...row.board, status, decline_reason: detail.board.decline_reason ?? null },
  };
}

/**
 * One board beside the table: the decision first (ask for permission, add its key, choose, approve,
 * or pause, check again and stop), then the facts, then each check in plain words with the quoted
 * sentence. "More options" opens the full board panel (config, proposals, the key).
 */
export function AdminPeek({ row: listed, onClose, onDeclined }: AdminPeekProps) {
  const { t } = useTranslation("sources");
  const [more, setMore] = useState<string | null>(null);
  const board = useBoard(listed?.board.id ?? null);
  const row = listed === undefined ? undefined : reconcile(listed, board.data);
  if (more !== null && row?.board.id === more) {
    return (
      <BoardPeek
        id={row.board.id}
        onClose={() => {
          setMore(null);
          onClose();
        }}
      />
    );
  }
  const detail = row === undefined ? undefined : board.data;
  return (
    <SidePeek
      open={row !== undefined}
      label={row?.board.name ?? t("admin.peek.eyebrow")}
      eyebrow={row === undefined ? t("admin.peek.eyebrow") : t(`admin.kind.${row.kind}`)}
      closeLabel={t("admin.peek.close")}
      onClose={onClose}
      testId="boards-admin-peek"
      closeTestId="boards-admin-peek-close"
    >
      {row === undefined ? null : (
        <Stack gap="lg" testId="boards-admin-peek-body">
          <Stack gap="sm">
            <Heading level={2} size="pane">
              {row.board.name}
            </Heading>
            <div>
              <StageLabel status={row.status} />
            </div>
            {row.board.note === undefined || row.board.note === null ? null : (
              <Text tone="secondary">{row.board.note}</Text>
            )}
            {row.status === "waiting" || row.status === "checking" ? (
              <WhyLine waiting={row.board.waiting} />
            ) : null}
          </Stack>
          {board.isError ? (
            <Tagged testId="boards-admin-peek-error">
              <Banner tone="danger" icon="warning" title={t("admin.loadFailed")} />
            </Tagged>
          ) : null}
          {detail === undefined && !board.isError ? <Skeleton width="60%" /> : null}
          {detail === undefined ? null : (
            <>
              <Decision
                key={row.board.id + row.status}
                row={row}
                detail={detail}
                onDeclined={onDeclined}
              />
              <Facts row={row} detail={detail} />
              <Checks detail={detail} />
              <Cluster>
                <Button
                  variant="ghost"
                  data-testid="boards-admin-more-options"
                  onClick={() => {
                    setMore(row.board.id);
                  }}
                >
                  {t("admin.peek.more")}
                </Button>
              </Cluster>
            </>
          )}
        </Stack>
      )}
    </SidePeek>
  );
}

/** An action that failed, beside its button, in plain words with what the server said. */
function ErrorLine({ testId, error }: { testId: string; error: unknown }) {
  const { t } = useTranslation("sources");
  const { code, detail } = boardErrorOf(error);
  const text = errorText(code, t);
  return (
    <Tagged testId={testId}>
      <Banner tone="danger" icon="warning" role="alert" title={text}>
        {detail}
      </Banner>
    </Tagged>
  );
}

/** What an action's problem means, in plain words; any other problem says it did not work. */
function errorText(code: BoardErrorCode, t: TFunction<"sources">): string {
  switch (code) {
    case "not_asked": {
      return t("admin.errors.not_asked");
    }
    case "not_waiting": {
      return t("admin.errors.not_waiting");
    }
    case "robots_disallowed": {
      return t("admin.errors.robots_disallowed");
    }
    case "needs_key": {
      return t("admin.errors.needs_key");
    }
    case "check_needed": {
      return t("admin.errors.check_needed");
    }
    default: {
      return t("admin.errors.generic");
    }
  }
}

function Decision({
  row,
  detail,
  onDeclined,
}: {
  row: AdminRow;
  detail: BoardDetail;
  onDeclined: (id: string) => void;
}) {
  switch (row.status) {
    case "waiting": {
      switch (row.need) {
        case "permission": {
          return <PermissionDecision row={row} onDeclined={onDeclined} />;
        }
        case "key": {
          return <KeyDecision detail={detail} onDeclined={onDeclined} />;
        }
        case "choice": {
          return <ChoiceDecision row={row} onDeclined={onDeclined} />;
        }
        default: {
          return <ApproveDecision row={row} onDeclined={onDeclined} />;
        }
      }
    }
    case "checking": {
      return <CheckingDecision row={row} />;
    }
    case "notAllowed": {
      return <NotAllowedDecision row={row} />;
    }
    case "reading":
    case "paused": {
      return <ReadingDecision row={row} onDeclined={onDeclined} />;
    }
  }
}

/** Decline in place: it is never read, and it says so before it happens. */
function DeclineButton({
  id,
  name,
  onDeclined,
}: {
  id: string;
  name: string;
  onDeclined: (id: string) => void;
}) {
  const { t } = useTranslation("sources");
  const action = useBoardAction(id);
  const [asking, setAsking] = useState(false);
  if (asking) {
    return (
      <Stack gap="sm">
        <InlineConfirm
          confirmLabel={t("admin.decline.confirm")}
          cancelLabel={t("admin.cancel")}
          busy={action.isPending}
          onConfirm={() => {
            action.mutate(
              { kind: "decline", reason: DECLINED_HERE },
              {
                onSuccess: () => {
                  onDeclined(id);
                },
              },
            );
          }}
          onCancel={() => {
            setAsking(false);
          }}
        >
          {t("admin.decline.sure", { name })}
        </InlineConfirm>
        {action.isError ? (
          <ErrorLine testId="boards-admin-decline-error" error={action.error} />
        ) : null}
      </Stack>
    );
  }
  return (
    <Button
      variant="secondary"
      data-testid="boards-admin-decline"
      onClick={() => {
        setAsking(true);
      }}
    >
      {t("admin.decline.button")}
    </Button>
  );
}

function PermissionDecision({
  row,
  onDeclined,
}: {
  row: AdminRow;
  onDeclined: (id: string) => void;
}) {
  const { t, i18n } = useTranslation("sources");
  const id = row.board.id;
  const email = usePermissionEmail(id);
  const asked = useMarkAsked(id);
  const answer = useAnswerRequest(id);
  const draft = email.data;
  const askedAt = draft?.asked_at ?? row.askedAt ?? null;
  return (
    <Section
      title={t("admin.permission.title")}
      description={t("admin.permission.description")}
      testId="boards-admin-permission"
    >
      <Stack gap="sm">
        {draft === undefined ? (
          <Skeleton width="80%" />
        ) : (
          <>
            <Text tone="secondary" size="sm">
              {draft.to === undefined || draft.to === null
                ? t("admin.permission.noContact")
                : t("admin.permission.to", { to: draft.to })}
            </Text>
            <CopyField
              label={t("admin.permission.email")}
              value={`Subject: ${draft.subject}\n\n${draft.body}`}
              copyLabel={t("admin.permission.copy")}
              copiedLabel={t("admin.permission.copied")}
              failedLabel={t("admin.permission.copyFailed")}
            />
          </>
        )}
        {askedAt === null ? (
          <Cluster>
            <Button
              variant="primary"
              icon="check"
              disabled={asked.isPending}
              data-testid="boards-admin-asked"
              onClick={() => {
                asked.mutate();
              }}
            >
              {t("admin.permission.sent")}
            </Button>
            <DeclineButton id={id} name={row.board.name} onDeclined={onDeclined} />
          </Cluster>
        ) : (
          <>
            <Tagged testId="boards-admin-asked-on">
              <Text tone="secondary">
                {t("admin.askedOn", { date: shortDate(askedAt, i18n.language) })}
              </Text>
            </Tagged>
            <Cluster>
              <Button
                variant="primary"
                icon="check"
                disabled={answer.isPending}
                data-testid="boards-admin-yes"
                onClick={() => {
                  answer.mutate({ answer: "yes" });
                }}
              >
                {t("admin.permission.yes")}
              </Button>
              <Button
                variant="secondary"
                disabled={answer.isPending}
                data-testid="boards-admin-no"
                onClick={() => {
                  answer.mutate(
                    { answer: "no" },
                    {
                      onSuccess: () => {
                        onDeclined(row.board.id);
                      },
                    },
                  );
                }}
              >
                {t("admin.permission.no")}
              </Button>
            </Cluster>
          </>
        )}
        {asked.isError ? <ErrorLine testId="boards-admin-asked-error" error={asked.error} /> : null}
        {answer.isError ? (
          <ErrorLine testId="boards-admin-answer-error" error={answer.error} />
        ) : null}
      </Stack>
    </Section>
  );
}

function KeyDecision({
  detail,
  onDeclined,
}: {
  detail: BoardDetail;
  onDeclined: (id: string) => void;
}) {
  const { t } = useTranslation("sources");
  const id = detail.board.id;
  const action = useBoardAction(id);
  const [values, setValues] = useState<Record<string, string>>({});
  const key = detail.key;
  if (key === undefined || key === null) {
    return null;
  }
  const filled = key.params.every((param) => (values[param] ?? "").trim() !== "");
  const save = () => {
    const body = Object.fromEntries(
      key.params.map((param) => [param, (values[param] ?? "").trim()]),
    );
    action.mutate(
      { kind: "key", values: body },
      {
        onSuccess: () => {
          setValues({});
          action.mutate({ kind: "check" });
        },
      },
    );
  };
  return (
    <Section
      title={t("admin.key.title")}
      description={t("admin.key.description")}
      testId="boards-admin-key"
    >
      <Stack gap="sm">
        <Steps
          label={t("admin.key.title")}
          steps={[
            <>
              {t("admin.key.open", { name: detail.board.name })}{" "}
              <ButtonLink
                variant="link"
                href={key.url}
                target="_blank"
                rel="noopener noreferrer"
                data-testid="boards-admin-key-get"
              >
                {t("admin.key.page")}
              </ButtonLink>
            </>,
            key.how,
            t("admin.key.paste"),
          ]}
        />
        {key.params.map((param) => (
          <TextField
            key={param}
            label={t("admin.key.field")}
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={values[param] ?? ""}
            onChange={(event) => {
              setValues((current) => ({ ...current, [param]: event.target.value }));
            }}
            data-testid={`boards-admin-key-${param}`}
          />
        ))}
        <Cluster>
          <Button
            variant="primary"
            icon="check"
            disabled={!filled || action.isPending}
            data-testid="boards-admin-key-save"
            onClick={save}
          >
            {t("admin.key.save")}
          </Button>
          <DeclineButton id={id} name={detail.board.name} onDeclined={onDeclined} />
        </Cluster>
        {action.isError ? <ErrorLine testId="boards-admin-key-error" error={action.error} /> : null}
      </Stack>
    </Section>
  );
}

function ChoiceDecision({ row, onDeclined }: { row: AdminRow; onDeclined: (id: string) => void }) {
  const { t } = useTranslation("sources");
  const action = useBoardAction(row.board.id);
  return (
    <Section
      title={t("admin.choice.title")}
      description={t("admin.choice.description")}
      testId="boards-admin-choice"
    >
      <Stack gap="sm">
        <Cluster>
          <Button
            variant="primary"
            disabled={action.isPending}
            data-testid="boards-admin-choice-yes"
            onClick={() => {
              action.mutate({ kind: "approve" });
            }}
          >
            {t("admin.choice.yes")}
          </Button>
          <Button
            variant="secondary"
            disabled={action.isPending}
            data-testid="boards-admin-choice-no"
            onClick={() => {
              action.mutate(
                { kind: "decline", reason: DECLINED_HERE },
                {
                  onSuccess: () => {
                    onDeclined(row.board.id);
                  },
                },
              );
            }}
          >
            {t("admin.choice.no")}
          </Button>
        </Cluster>
        {action.isError ? (
          <ErrorLine testId="boards-admin-choice-error" error={action.error} />
        ) : null}
      </Stack>
    </Section>
  );
}

function ApproveDecision({ row, onDeclined }: { row: AdminRow; onDeclined: (id: string) => void }) {
  const { t } = useTranslation("sources");
  const action = useBoardAction(row.board.id);
  return (
    <Section
      title={t("admin.approve.title")}
      description={t("admin.approve.description")}
      testId="boards-admin-approve-section"
    >
      <Stack gap="sm">
        <Cluster>
          <Button
            variant="primary"
            icon="check"
            disabled={action.isPending}
            data-testid="boards-admin-approve"
            onClick={() => {
              action.mutate({ kind: "approve" });
            }}
          >
            {t("admin.approve.button")}
          </Button>
          <DeclineButton id={row.board.id} name={row.board.name} onDeclined={onDeclined} />
        </Cluster>
        {action.isError ? (
          <ErrorLine testId="boards-admin-approve-error" error={action.error} />
        ) : null}
      </Stack>
    </Section>
  );
}

function CheckingDecision({ row }: { row: AdminRow }) {
  const { t } = useTranslation("sources");
  const action = useBoardAction(row.board.id);
  return (
    <Section
      title={t("admin.checking.title")}
      description={t("admin.checking.description")}
      testId="boards-admin-checking"
    >
      <Stack gap="sm">
        <Cluster>
          <Button
            variant="secondary"
            disabled={action.isPending}
            data-testid="boards-admin-check"
            onClick={() => {
              action.mutate({ kind: "check" });
            }}
          >
            {t("admin.reading.check")}
          </Button>
        </Cluster>
        {action.isError ? (
          <ErrorLine testId="boards-admin-check-error" error={action.error} />
        ) : null}
      </Stack>
    </Section>
  );
}

function NotAllowedDecision({ row }: { row: AdminRow }) {
  const { t } = useTranslation("sources");
  const reopen = useReopen(row.board.id);
  const reason = row.board.decline_reason ?? row.detail;
  const declined = row.board.status === "declined";
  return (
    <Section
      title={declined ? t("admin.declined") : t("admin.notAllowed.title")}
      description={declined ? undefined : t("admin.notAllowed.description")}
      testId="boards-admin-not-allowed"
    >
      <Stack gap="sm">
        {reason === undefined ? null : <Text tone="secondary">{`\u201C${reason}\u201D`}</Text>}
        {declined ? (
          <Cluster>
            <Button
              variant="secondary"
              disabled={reopen.isPending}
              data-testid="boards-admin-peek-undo"
              onClick={() => {
                reopen.mutate();
              }}
            >
              {t("admin.undo")}
            </Button>
          </Cluster>
        ) : null}
        {reopen.isError ? (
          <ErrorLine testId="boards-admin-undo-error" error={reopen.error} />
        ) : null}
      </Stack>
    </Section>
  );
}

function ReadingDecision({ row, onDeclined }: { row: AdminRow; onDeclined: (id: string) => void }) {
  const { t } = useTranslation("sources");
  const action = useBoardAction(row.board.id);
  const [stopping, setStopping] = useState(false);
  const run = (next: BoardAction) => {
    action.mutate(next);
  };
  return (
    <Section
      title={row.status === "paused" ? t("admin.paused.title") : t("admin.reading.title")}
      testId="boards-admin-reading"
    >
      <Stack gap="sm">
        <Cluster>
          {row.status === "paused" ? (
            <Button
              variant="secondary"
              disabled={action.isPending}
              data-testid="boards-admin-resume"
              onClick={() => {
                run({ kind: "approve" });
              }}
            >
              {t("admin.paused.resume")}
            </Button>
          ) : (
            <Button
              variant="secondary"
              disabled={action.isPending}
              data-testid="boards-admin-pause"
              onClick={() => {
                run({ kind: "pause" });
              }}
            >
              {t("admin.reading.pause")}
            </Button>
          )}
          <Button
            variant="secondary"
            disabled={action.isPending}
            data-testid="boards-admin-check"
            onClick={() => {
              run({ kind: "check" });
            }}
          >
            {t("admin.reading.check")}
          </Button>
          <Button
            variant="secondary"
            disabled={action.isPending}
            data-testid="boards-admin-stop"
            onClick={() => {
              setStopping(true);
            }}
          >
            {t("admin.reading.stop")}
          </Button>
        </Cluster>
        {stopping ? (
          <Tagged testId="boards-admin-stop-confirm">
            <InlineConfirm
              confirmLabel={t("admin.reading.stopConfirm")}
              cancelLabel={t("admin.cancel")}
              busy={action.isPending}
              onConfirm={() => {
                action.mutate(
                  { kind: "decline", reason: DECLINED_HERE },
                  {
                    onSuccess: () => {
                      onDeclined(row.board.id);
                    },
                  },
                );
              }}
              onCancel={() => {
                setStopping(false);
              }}
            >
              {t("admin.reading.stopSure", { name: row.board.name })}
            </InlineConfirm>
          </Tagged>
        ) : null}
        {action.isError ? (
          <ErrorLine testId="boards-admin-action-error" error={action.error} />
        ) : null}
      </Stack>
    </Section>
  );
}

/** The facts: the jobs it has, where, how it is read, where it was found and when it was checked. */
function Facts({ row, detail }: { row: AdminRow; detail: BoardDetail }) {
  const { t, i18n } = useTranslation("sources");
  const { board, report } = detail;
  const finder = board;
  const found = finder.found_through ?? t(`admin.origin.${board.origin}`);
  const checked = report?.checked_at ?? null;
  return (
    <PropertyList testId="boards-admin-facts">
      <Property label={t("admin.facts.jobs")}>
        {board.last_fetch_jobs === undefined || board.last_fetch_jobs === null
          ? t("admin.none")
          : new Intl.NumberFormat(i18n.language).format(board.last_fetch_jobs)}
      </Property>
      <Property label={t("admin.facts.where")}>{board.country ?? t("admin.worldwide")}</Property>
      <Property label={t("admin.facts.how")}>{howRead(board.kind, t)}</Property>
      <Property label={t("admin.facts.found")}>{found}</Property>
      <Property label={t("admin.facts.checked")}>
        {row.status === "checking" && checked === null
          ? t("admin.facts.now")
          : checked === null
            ? t("admin.facts.never")
            : formatWhen(checked, i18n.language).text}
      </Property>
    </PropertyList>
  );
}

type CheckState = "ok" | "part" | "none";

interface CheckLine {
  id: string;
  state: CheckState;
  text: string;
  quote?: string | undefined;
}

/** What the app checked, in plain words, each with the sentence it found where it has one. */
function checksOf(detail: BoardDetail, t: TFunction<"sources">): CheckLine[] {
  const lines: CheckLine[] = [];
  const blockers = detail.blockers;
  const finder = detail.board;
  const checked = detail.report?.checked_at !== undefined && detail.report.checked_at !== null;
  lines.push(
    detail.endpoint === undefined || detail.endpoint === null
      ? { id: "feed", state: "part", text: t("admin.checks.feedWait") }
      : { id: "feed", state: "ok", text: t("admin.checks.feedOk") },
  );
  if (blockers.some((b) => b.code === "robots_disallowed")) {
    lines.push({ id: "site", state: "none", text: t("admin.checks.siteNo") });
  } else if (checked) {
    lines.push({ id: "site", state: "ok", text: t("admin.checks.siteOk") });
  } else {
    lines.push({ id: "site", state: "part", text: t("admin.checks.siteWait") });
  }
  const forbidding = blockers.find((b) => b.code === "terms_forbid");
  if (forbidding !== undefined) {
    lines.push({
      id: "rules",
      state: "none",
      text: t("admin.checks.rulesNo"),
      quote: finder.terms_quote ?? forbidding.detail,
    });
  } else if (checked) {
    lines.push({ id: "rules", state: "ok", text: t("admin.checks.rulesOk") });
  } else {
    lines.push({ id: "rules", state: "part", text: t("admin.checks.rulesWait") });
  }
  if (detail.key !== undefined && detail.key !== null) {
    lines.push(
      detail.key.set
        ? { id: "key", state: "ok", text: t("admin.checks.keyOk") }
        : { id: "key", state: "none", text: t("admin.checks.keyNo") },
    );
  }
  return lines;
}

function Checks({ detail }: { detail: BoardDetail }) {
  const { t } = useTranslation("sources");
  const lines = checksOf(detail, t);
  return (
    <Section title={t("admin.checks.title")} testId="boards-admin-checks">
      <Stack gap="sm">
        {lines.map((line) => (
          <Tagged key={line.id} testId={`boards-admin-check-${line.id}`}>
            <Cluster>
              <StatusMark state={line.state} label={t(`admin.checks.state.${line.state}`)} />
              <Stack gap="sm">
                <Text>{line.text}</Text>
                {line.quote === undefined ? null : (
                  <Text tone="secondary">{`“${line.quote}”`}</Text>
                )}
              </Stack>
            </Cluster>
          </Tagged>
        ))}
      </Stack>
    </Section>
  );
}
