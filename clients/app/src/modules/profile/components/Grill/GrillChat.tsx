import { useState, type KeyboardEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Chat,
  ChatBubble,
  Cluster,
  Placeholder,
  Progress,
  Stack,
  Tagged,
  Text,
  TextArea,
} from "~/lib/ui";

import {
  MAX_ANSWER,
  useAnswerGrill,
  useDoneGrill,
  useSkipGrill,
  useStartGrill,
  type GrillTurn,
  type GrillView,
} from "../../api/grill";
import { FactsTaken } from "./FactsTaken";
import { errorCode, firstError } from "./labels";

export interface GrillChatProps {
  view: GrillView;
  /** The role's name, for "Saved to …". */
  role: string;
  /** The month's AI spend line, under the composer. */
  spend?: ReactNode;
}

/** The conversation of one role, and what the person can do next. */
export function GrillChat({ view, role, spend }: GrillChatProps) {
  const { t, i18n } = useTranslation("profile");
  const start = useStartGrill(i18n.language);
  const answer = useAnswerGrill(i18n.language);
  const skip = useSkipGrill(i18n.language);
  const done = useDoneGrill();
  const [text, setText] = useState("");
  const position = view.position;
  const busy = start.isPending || answer.isPending || skip.isPending || done.isPending;
  const failure = firstError(start.error, answer.error, skip.error, done.error);
  const last = view.turns.at(-1);

  const clearErrors = () => {
    start.reset();
    answer.reset();
    skip.reset();
    done.reset();
  };
  const send = () => {
    const trimmed = text.trim();
    if (trimmed === "" || busy || view.question === null || view.question === undefined) {
      return;
    }
    clearErrors();
    answer.mutate(
      { position, text: trimmed },
      {
        onSuccess: () => {
          setText("");
        },
      },
    );
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      send();
    }
  };
  const begin = () => {
    clearErrors();
    start.mutate(position);
  };

  const renderTurn = (turn: GrillTurn) => {
    switch (turn.kind) {
      case "question": {
        return (
          <ChatBubble
            key={turn.seq}
            kind={turn.push ? "push" : "question"}
            label={t(turn.push ? "grill.labels.push" : "grill.labels.question")}
            testId={`grill-turn-${String(turn.seq)}`}
          >
            {turn.text}
          </ChatBubble>
        );
      }
      case "answer": {
        const facts = view.proposals.filter((fact) => fact.turn_seq === turn.seq);
        return (
          <Stack key={turn.seq} gap="sm">
            <ChatBubble
              kind="answer"
              label={t("grill.labels.answer")}
              testId={`grill-turn-${String(turn.seq)}`}
            >
              {turn.text}
            </ChatBubble>
            {turn.dropped > 0 ? (
              <Tagged testId={`grill-left-out-${String(turn.seq)}`}>
                <Text tone="secondary" size="sm">
                  {t("grill.facts.leftOut", { count: turn.dropped })}
                </Text>
              </Tagged>
            ) : null}
            {facts.length > 0 ? (
              <FactsTaken view={view} facts={facts} turn={turn.seq} role={role} />
            ) : null}
          </Stack>
        );
      }
      default: {
        return (
          <Tagged key={turn.seq} testId={`grill-note-${String(turn.seq)}`}>
            <Text tone="tertiary" size="sm">
              {t(turn.text === "skipped" ? "grill.skippedNote" : "grill.doneNote")}
            </Text>
          </Tagged>
        );
      }
    }
  };

  const asking = view.question !== null && view.question !== undefined;
  const covered = view.status === "covered";

  return (
    <Stack gap="md" testId="grill-chat">
      {view.turns.length > 0 ? (
        <Chat label={t("grill.logLabel")} testId="grill-log">
          {view.turns.map(renderTurn)}
        </Chat>
      ) : null}

      {answer.isPending || skip.isPending || start.isPending ? (
        <Tagged testId="grill-thinking">
          <Progress
            label={t(start.isPending ? "grill.starting" : "grill.answering")}
            value={0}
            max={0}
          >
            {t(start.isPending ? "grill.starting" : "grill.answering")}
          </Progress>
        </Tagged>
      ) : null}

      {failure === undefined ? null : (
        <Tagged testId="grill-error">
          <Banner
            tone="danger"
            icon="warning"
            role="alert"
            title={t(`grill.errors.${errorCode(failure)}`)}
          />
        </Tagged>
      )}

      {asking ? (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            send();
          }}
        >
          <Stack gap="sm">
            <TextArea
              label={t("grill.answerLabel")}
              hint={t("grill.answerHint")}
              placeholder={t("grill.answerPlaceholder")}
              rows={5}
              maxLength={MAX_ANSWER}
              value={text}
              data-testid="grill-answer"
              onChange={(event) => {
                setText(event.target.value);
              }}
              onKeyDown={onKeyDown}
            />
            <Cluster>
              <Button
                variant="primary"
                type="submit"
                icon="arrow-right"
                disabled={busy || text.trim() === ""}
                data-testid="grill-send"
              >
                {t("grill.answer")}
              </Button>
              <Button
                variant="ghost"
                disabled={busy}
                data-testid="grill-skip"
                onClick={() => {
                  clearErrors();
                  skip.mutate(position);
                }}
              >
                {t("grill.skip")}
              </Button>
              <Button
                variant="link"
                disabled={busy}
                data-testid="grill-done"
                onClick={() => {
                  clearErrors();
                  done.mutate(position);
                }}
              >
                {t("grill.done")}
              </Button>
            </Cluster>
          </Stack>
        </form>
      ) : covered ? (
        <Tagged testId="grill-covered">
          <Banner icon="check" role="status" title={t("grill.coveredBanner")}>
            {t("grill.coveredLead")}
          </Banner>
        </Tagged>
      ) : view.turns.length === 0 ? (
        <Placeholder
          testId="grill-start-panel"
          icon="tasks"
          title={t("grill.startTitle")}
          action={
            <Button
              variant="primary"
              icon="arrow-right"
              disabled={busy}
              data-testid="grill-start"
              onClick={begin}
            >
              {t("grill.start")}
            </Button>
          }
        >
          {t("grill.startLead")}
        </Placeholder>
      ) : (
        <Stack gap="sm">
          {last?.kind === "note" && last.text === "skipped" ? (
            <Tagged testId="grill-stopped">
              <Banner icon="check" role="status" title={t("grill.stoppedBanner")} />
            </Tagged>
          ) : null}
          <Cluster>
            <Button
              variant="primary"
              icon="arrow-right"
              disabled={busy}
              data-testid="grill-continue"
              onClick={begin}
            >
              {t("grill.continue")}
            </Button>
          </Cluster>
        </Stack>
      )}

      <Stack gap="sm">
        <Tagged testId="grill-cost">
          <Text tone="secondary" size="sm">
            {t("grill.costLine", { n: view.calls_today, cap: view.daily_cap })}
          </Text>
        </Tagged>
        {spend}
      </Stack>
    </Stack>
  );
}
