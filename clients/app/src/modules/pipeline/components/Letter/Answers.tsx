import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Button,
  Cluster,
  Label,
  RowItem,
  RowList,
  Section,
  Stack,
  Tagged,
  Text,
  TextArea,
  type LabelHue,
} from "~/lib/ui";

import type { LetterAnswer, LetterDraft, LetterTag } from "../../api/letter";
import { ReasonText } from "./SentenceFix";

/** Longest answer the server takes. */
export const MAX_ANSWER = 1200;

const TAG_HUES: Record<LetterTag, LabelHue> = {
  company_fact: "green",
  from_profile: "gray",
  no_claims: "blue",
};

/** What the person can do with the job's questions. */
export interface AnswerActions {
  editable: boolean;
  busy: boolean;
  /** Use the person's own words as the answer. */
  write: (answer: string, text: string) => void;
  /** Leave the question out. */
  remove: (answer: string) => void;
}

/** Puts `text` on the clipboard; whether it worked. */
async function copy(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

function AnswerRow({ answer, actions }: { answer: LetterAnswer; actions: AnswerActions }) {
  const { t } = useTranslation("pipeline");
  const flagged = answer.flags.length > 0;
  const [writing, setWriting] = useState(false);
  const [text, setText] = useState(answer.text);
  const [copied, setCopied] = useState(false);
  const empty = answer.text.trim() === "";
  const open = writing || (flagged && empty);
  const id = answer.id;
  return (
    <RowItem
      testId={`letter-answer-${id}`}
      title={answer.question}
      subtitle={
        <Stack gap="sm">
          {flagged ? (
            <Tagged testId={`letter-answer-flag-${id}`}>
              <Stack gap="sm">
                {answer.flags.map((flag) => (
                  <Text key={flag.kind} tone="secondary" size="sm">
                    <ReasonText reason={flag} />
                  </Text>
                ))}
              </Stack>
            </Tagged>
          ) : null}
          {empty ? null : (
            <Tagged testId={`letter-answer-text-${id}`}>
              <Text>{answer.text}</Text>
            </Tagged>
          )}
          {open && actions.editable ? (
            <Stack gap="sm">
              <TextArea
                label={t("letter.questions.yourAnswer")}
                rows={3}
                maxLength={MAX_ANSWER}
                value={text}
                onChange={(event) => {
                  setText(event.target.value);
                }}
                data-testid={`letter-answer-input-${id}`}
              />
              <Cluster>
                <Button
                  variant="primary"
                  disabled={actions.busy || text.trim() === ""}
                  data-testid={`letter-answer-save-${id}`}
                  onClick={() => {
                    actions.write(id, text.trim());
                    setWriting(false);
                  }}
                >
                  {t("letter.questions.useMine")}
                </Button>
              </Cluster>
            </Stack>
          ) : null}
        </Stack>
      }
      meta={
        <>
          {answer.own && !empty ? (
            <Tagged testId={`letter-answer-own-${id}`}>
              <Label hue="green">{t("letter.questions.own")}</Label>
            </Tagged>
          ) : null}
          {answer.tags.map((tag) => (
            <Label key={tag} hue={TAG_HUES[tag]}>
              {t(`letter.tags.${tag}`)}
            </Label>
          ))}
        </>
      }
      actions={
        <>
          {empty ? null : (
            <Button
              variant="secondary"
              data-testid={`letter-answer-copy-${id}`}
              onClick={() => {
                void copy(answer.text).then(setCopied);
              }}
            >
              {copied ? t("letter.questions.copied") : t("letter.questions.copy")}
            </Button>
          )}
          {actions.editable && !open ? (
            <Button
              variant="secondary"
              disabled={actions.busy}
              data-testid={`letter-answer-edit-${id}`}
              onClick={() => {
                setText(answer.text);
                setWriting(true);
              }}
            >
              {t("letter.questions.write")}
            </Button>
          ) : null}
          {actions.editable ? (
            <Button
              variant="ghost"
              disabled={actions.busy}
              data-testid={`letter-answer-remove-${id}`}
              onClick={() => {
                actions.remove(id);
              }}
            >
              {t("letter.questions.leaveOut")}
            </Button>
          ) : null}
        </>
      }
    />
  );
}

/**
 * The job's own questions and their answers: text to copy into the
 * application, not part of the PDF. One the profile can't support comes back
 * empty, flagged, with a box for the person's own answer.
 */
export function Answers({ draft, actions }: { draft: LetterDraft; actions: AnswerActions }) {
  const { t } = useTranslation("pipeline");
  if (draft.answers.length === 0) {
    return null;
  }
  return (
    <Section
      title={t("letter.questions.title")}
      description={t("letter.questions.lead")}
      testId="letter-questions"
    >
      <RowList label={t("letter.questions.title")}>
        {draft.answers.map((answer) => (
          <AnswerRow key={`${answer.id}-${answer.text}`} answer={answer} actions={actions} />
        ))}
      </RowList>
    </Section>
  );
}
