import { useState } from "react";
import { useTranslation } from "react-i18next";

import {
  Button,
  Cluster,
  CvLine,
  Disclosure,
  Label,
  Marked,
  Stack,
  Tagged,
  Text,
  TextArea,
  type LabelHue,
} from "~/lib/ui";

import { canConfirm, type CvLine as LineView, type CvMark, type CvReason } from "../../api/cv";

/** Longest line the server takes, by what the line is. */
export const MAX_LINE = 700;
/** Longest fact "It's true" adds to the profile. */
export const MAX_FACT = 300;

const MARK_HUES: Record<CvMark, LabelHue> = {
  as_in_profile: "gray",
  moved_up: "blue",
  reworded: "purple",
  edited: "yellow",
};

/** What the person can do with a line. */
export interface LineActions {
  /** The CV can change (a draft, not an approved version). */
  editable: boolean;
  busy: boolean;
  edit: (line: string, text: string) => void;
  remove: (line: string) => void;
  confirm: (line: string, text?: string) => void;
}

/** Why a line is flagged, in words. */
export function ReasonText({ reason }: { reason: CvReason }) {
  const { t } = useTranslation("pipeline");
  switch (reason.kind) {
    case "claim_not_in_sources": {
      return <>{t("cv.flag.claim", { claim: reason.claim })}</>;
    }
    case "no_source": {
      return <>{t("cv.flag.noSource")}</>;
    }
    case "company_fact_outside_summary": {
      return <>{t("cv.flag.company")}</>;
    }
    case "skill_not_on_profile": {
      return <>{t("cv.flag.skill", { skill: reason.skill })}</>;
    }
  }
}

/** Just the actions for a flagged line: Edit, the person's words, Remove. */
export function FlagActions({
  line,
  where,
  actions,
  flags,
  initial,
}: {
  line: string;
  /** The position the person's words go on, in words ("Senior Engineer · Example Org"). */
  where: string | undefined;
  actions: LineActions;
  flags: readonly CvReason[];
  initial: string;
}) {
  const { t } = useTranslation("pipeline");
  const [mode, setMode] = useState<"none" | "edit" | "confirm">("none");
  const [text, setText] = useState(initial);
  const [words, setWords] = useState("");
  const close = () => {
    setMode("none");
  };
  return (
    <Stack gap="sm">
      <Tagged testId={`cv-flag-${line}`}>
        <Stack gap="sm">
          {flags.map((flag) => (
            <Text
              key={`${flag.kind}-${"claim" in flag ? flag.claim : ""}`}
              weight="medium"
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
            data-testid={`cv-edit-${line}`}
            onClick={() => {
              setText(initial);
              setMode("edit");
            }}
          >
            {t("cv.actions.edit")}
          </Button>
          {canConfirm(flags) ? (
            <Button
              variant="secondary"
              disabled={actions.busy}
              data-testid={`cv-confirm-${line}`}
              onClick={() => {
                setMode("confirm");
              }}
            >
              {t("cv.actions.itsTrue")}
            </Button>
          ) : null}
          <Button
            variant="ghost"
            disabled={actions.busy}
            data-testid={`cv-remove-${line}`}
            onClick={() => {
              actions.remove(line);
            }}
          >
            {t("cv.actions.removeLine")}
          </Button>
        </Cluster>
      ) : null}
      {mode === "edit" ? (
        <Stack gap="sm">
          <TextArea
            label={t("cv.actions.editLabel")}
            rows={3}
            maxLength={MAX_LINE}
            value={text}
            onChange={(event) => {
              setText(event.target.value);
            }}
            data-testid={`cv-edit-input-${line}`}
          />
          <Cluster>
            <Button
              variant="primary"
              disabled={actions.busy || text.trim() === ""}
              data-testid={`cv-edit-save-${line}`}
              onClick={() => {
                actions.edit(line, text.trim());
                close();
              }}
            >
              {t("cv.actions.save")}
            </Button>
            <Button variant="ghost" data-testid={`cv-edit-cancel-${line}`} onClick={close}>
              {t("cv.actions.cancel")}
            </Button>
          </Cluster>
        </Stack>
      ) : null}
      {mode === "confirm" ? (
        <Stack gap="sm">
          <TextArea
            label={t("cv.actions.confirmLabel")}
            hint={
              where === undefined
                ? t("cv.actions.confirmHintNoRole")
                : t("cv.actions.confirmHint", { role: where })
            }
            rows={3}
            maxLength={MAX_FACT}
            value={words}
            onChange={(event) => {
              setWords(event.target.value);
            }}
            data-testid={`cv-confirm-input-${line}`}
          />
          <Cluster>
            <Button
              variant="primary"
              disabled={actions.busy || words.trim() === ""}
              data-testid={`cv-confirm-save-${line}`}
              onClick={() => {
                actions.confirm(line, words.trim());
                close();
              }}
            >
              {t("cv.actions.confirmSave")}
            </Button>
            <Button variant="ghost" data-testid={`cv-confirm-cancel-${line}`} onClick={close}>
              {t("cv.actions.cancel")}
            </Button>
          </Cluster>
        </Stack>
      ) : null}
    </Stack>
  );
}

/** What a source says, with the line's claims picked out. */
function FromProfile({ line }: { line: LineView }) {
  const { t } = useTranslation("pipeline");
  if (line.sources.length === 0) {
    return null;
  }
  return (
    <Tagged testId={`cv-from-${line.id}`}>
      <Stack gap="sm">
        <Text tone="secondary" size="sm" weight="medium">
          {t("cv.from.title")}
        </Text>
        {line.sources.map((source) => (
          <Text key={source.id} tone="secondary" size="sm" as="div">
            <Marked text={source.text} words={line.claims} />
            {source.kind === "company" && source.url !== undefined && source.url !== null ? (
              <> {t("cv.from.companyPage", { page: shownPage(source.url) })}</>
            ) : source.role === undefined || source.role === null ? null : (
              <> · {source.role}</>
            )}
          </Text>
        ))}
      </Stack>
    </Tagged>
  );
}

function shownPage(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.host.replace(/^www\./, "")}${parsed.pathname === "/" ? "" : parsed.pathname}`;
  } catch {
    return url;
  }
}

/**
 * One line of the CV: its text, how it relates to the profile ("Reworded"
 * opens "From your profile: …"), and, when it can't be traced, why and what
 * to do about it.
 */
export function LineRow({
  line,
  actions,
  where,
}: {
  line: LineView;
  actions: LineActions;
  /** The position it is about, for "It's true". */
  where: string | undefined;
}) {
  const { t } = useTranslation("pipeline");
  const flagged = line.flags.length > 0;
  const detail = flagged ? (
    <Stack gap="sm">
      <FlagActions
        line={line.id}
        where={where}
        actions={actions}
        flags={line.flags}
        initial={line.text}
      />
      {line.sources.length === 0 ? null : (
        <Tagged testId={`cv-from-toggle-${line.id}`}>
          <Disclosure summary={t("cv.from.show")}>
            <FromProfile line={line} />
          </Disclosure>
        </Tagged>
      )}
    </Stack>
  ) : line.mark === "reworded" || line.mark === "edited" ? (
    <Tagged testId={`cv-from-toggle-${line.id}`}>
      <Disclosure summary={t("cv.from.show")}>
        <FromProfile line={line} />
      </Disclosure>
    </Tagged>
  ) : undefined;
  return (
    <CvLine
      id={`cv-line-${line.id}`}
      flagged={flagged}
      testId={`cv-line-${line.id}`}
      mark={
        <Tagged testId={`cv-line-mark-${line.id}`}>
          <Label hue={flagged ? "red" : MARK_HUES[line.mark]}>
            {flagged ? t("cv.flag.label") : t(`cv.marks.${line.mark}`)}
          </Label>
        </Tagged>
      }
      detail={detail}
    >
      {line.text}
    </CvLine>
  );
}
