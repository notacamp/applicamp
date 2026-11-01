import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Button, Cluster, Stack, Tagged, Text, TextArea } from "~/lib/ui";

import type { LetterReason, LetterSentence } from "../../api/letter";

/** Longest sentence the server takes. */
export const MAX_SENTENCE = 400;
/** Longest fact "It's true" adds to the profile. */
export const MAX_FACT = 300;

/** What the person can do with a sentence. */
export interface SentenceActions {
  /** The letter can change (a draft, not an approved version). */
  editable: boolean;
  busy: boolean;
  edit: (sentence: string, text: string) => void;
  remove: (sentence: string) => void;
  confirm: (sentence: string, text: string) => void;
}

/** Why a sentence or an answer is flagged, in words. */
export function ReasonText({ reason }: { reason: LetterReason }) {
  const { t } = useTranslation("pipeline");
  switch (reason.kind) {
    case "claim_not_in_sources": {
      return <>{t("letter.flag.claim", { claim: reason.claim })}</>;
    }
    case "no_source": {
      return <>{t("letter.flag.noSource")}</>;
    }
    case "company_only_about_you": {
      return <>{t("letter.flag.companyOnly")}</>;
    }
    case "superlative": {
      return <>{t("letter.flag.superlative", { word: reason.word })}</>;
    }
    case "not_in_profile": {
      return <>{t("letter.flag.notInProfile")}</>;
    }
  }
}

/**
 * A sentence that can't be traced: the sentence, why, and what to do about
 * it: It's true (the person's words join the profile), Edit, Remove. It is
 * never dropped.
 */
export function SentenceFix({
  sentence,
  actions,
  where,
}: {
  sentence: LetterSentence;
  actions: SentenceActions;
  /** What the person's words are saved to, in words ("your profile"). */
  where: string;
}) {
  const { t } = useTranslation("pipeline");
  const [mode, setMode] = useState<"none" | "edit" | "confirm">("none");
  const [text, setText] = useState(sentence.text);
  const [words, setWords] = useState("");
  const id = sentence.id;
  const close = () => {
    setMode("none");
  };
  return (
    <Stack gap="sm">
      <Tagged testId={`letter-flag-${id}`}>
        <Stack gap="sm">
          <Text weight="medium" size="sm">
            “{sentence.text}”
          </Text>
          {sentence.flags.map((flag) => (
            <Text
              key={`${flag.kind}-${"claim" in flag ? flag.claim : ""}-${"word" in flag ? flag.word : ""}`}
              tone="secondary"
              size="sm"
            >
              <ReasonText reason={flag} />
            </Text>
          ))}
        </Stack>
      </Tagged>
      {actions.editable && mode === "none" ? (
        <Cluster>
          <Button
            variant="secondary"
            disabled={actions.busy}
            data-testid={`letter-confirm-${id}`}
            onClick={() => {
              setMode("confirm");
            }}
          >
            {t("letter.actions.itsTrue")}
          </Button>
          <Button
            variant="secondary"
            disabled={actions.busy}
            data-testid={`letter-edit-${id}`}
            onClick={() => {
              setText(sentence.text);
              setMode("edit");
            }}
          >
            {t("letter.actions.edit")}
          </Button>
          <Button
            variant="ghost"
            disabled={actions.busy}
            data-testid={`letter-remove-${id}`}
            onClick={() => {
              actions.remove(id);
            }}
          >
            {t("letter.actions.remove")}
          </Button>
        </Cluster>
      ) : null}
      {mode === "edit" ? (
        <Stack gap="sm">
          <TextArea
            label={t("letter.actions.editLabel")}
            rows={3}
            maxLength={MAX_SENTENCE}
            value={text}
            onChange={(event) => {
              setText(event.target.value);
            }}
            data-testid={`letter-edit-input-${id}`}
          />
          <Cluster>
            <Button
              variant="primary"
              disabled={actions.busy || text.trim() === ""}
              data-testid={`letter-edit-save-${id}`}
              onClick={() => {
                actions.edit(id, text.trim());
                close();
              }}
            >
              {t("letter.actions.save")}
            </Button>
            <Button variant="ghost" data-testid={`letter-edit-cancel-${id}`} onClick={close}>
              {t("letter.actions.cancel")}
            </Button>
          </Cluster>
        </Stack>
      ) : null}
      {mode === "confirm" ? (
        <Stack gap="sm">
          <TextArea
            label={t("letter.actions.confirmLabel")}
            hint={t("letter.actions.confirmHint", { where })}
            rows={3}
            maxLength={MAX_FACT}
            value={words}
            onChange={(event) => {
              setWords(event.target.value);
            }}
            data-testid={`letter-confirm-input-${id}`}
          />
          <Cluster>
            <Button
              variant="primary"
              disabled={actions.busy || words.trim() === ""}
              data-testid={`letter-confirm-save-${id}`}
              onClick={() => {
                actions.confirm(id, words.trim());
                close();
              }}
            >
              {t("letter.actions.confirmSave")}
            </Button>
            <Button variant="ghost" data-testid={`letter-confirm-cancel-${id}`} onClick={close}>
              {t("letter.actions.cancel")}
            </Button>
          </Cluster>
        </Stack>
      ) : null}
    </Stack>
  );
}
