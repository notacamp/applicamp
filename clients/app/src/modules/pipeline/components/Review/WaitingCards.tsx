import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Cluster, Stack, Tagged, Text, TextArea } from "~/lib/ui";

import type { CvWaiting } from "../../api/cv";
import { MAX_LINE, type LineActions } from "./LineRow";

/** One question the app could not answer for the person: Yes, keep it / Change it / Remove it. */
function WaitingCard({ item, actions }: { item: CvWaiting; actions: LineActions }) {
  const { t } = useTranslation("pipeline");
  const [changing, setChanging] = useState(false);
  // "Change it" starts from the person's own words, never the model's.
  const quote = item.quote ?? undefined;
  const [text, setText] = useState(quote ?? "");
  const skill = item.kind === "skill";
  const proposal = item.proposal ?? undefined;
  const company = item.company ?? undefined;
  const question = item.question ?? undefined;
  // Where the line comes from, in the person's own words.
  const where =
    skill || company === undefined || quote === undefined
      ? undefined
      : t("cv.waiting.toldUs", { company, quote });
  // What a yes keeps, word for word, or why there is no yes.
  let keeps: string;
  if (skill) {
    keeps = t("cv.waiting.skillWhere");
  } else if (proposal !== undefined) {
    keeps = t("cv.waiting.keeps", { proposal });
  } else if (item.cannot_keep === "other_position") {
    keeps = t("cv.waiting.cannotKeepOther");
  } else if (item.cannot_keep === "too_long") {
    keeps = t("cv.waiting.cannotKeepLong");
  } else {
    keeps = t("cv.waiting.cannotKeep");
  }
  const id = item.line_id;
  return (
    <Tagged testId={`cv-question-${id}`}>
      <Banner
        title={
          <Tagged testId={`cv-question-text-${id}`}>
            <span>
              {skill
                ? t("cv.waiting.skillQuestion", { skill: item.text })
                : t("cv.waiting.question", { text: item.text })}
            </span>
          </Tagged>
        }
      >
        <Stack gap="sm">
          {question === undefined ? null : (
            <Tagged testId={`cv-question-ask-${id}`}>
              <Text>{question}</Text>
            </Tagged>
          )}
          {where === undefined ? null : (
            <Tagged testId={`cv-question-where-${id}`}>
              <Text tone="secondary" size="sm">
                {where}
              </Text>
            </Tagged>
          )}
          <Tagged testId={`cv-question-keeps-${id}`}>
            <Text tone="secondary" size="sm">
              {keeps}
            </Text>
          </Tagged>
          {!actions.editable || changing ? null : (
            <Cluster>
              {proposal === undefined ? null : (
                <Button
                  variant="primary"
                  disabled={actions.busy}
                  data-testid={`cv-question-yes-${id}`}
                  onClick={() => {
                    actions.keep(id);
                  }}
                >
                  {t("cv.waiting.yes")}
                </Button>
              )}
              {skill ? null : (
                <Button
                  variant="secondary"
                  disabled={actions.busy}
                  data-testid={`cv-question-change-${id}`}
                  onClick={() => {
                    setText(quote ?? "");
                    setChanging(true);
                  }}
                >
                  {t("cv.waiting.change")}
                </Button>
              )}
              <Button
                variant="ghost"
                disabled={actions.busy}
                data-testid={`cv-question-remove-${id}`}
                onClick={() => {
                  actions.remove(id);
                }}
              >
                {t("cv.waiting.remove")}
              </Button>
            </Cluster>
          )}
          {changing ? (
            <Stack gap="sm">
              <TextArea
                label={t("cv.waiting.changeLabel")}
                rows={3}
                maxLength={MAX_LINE}
                value={text}
                onChange={(event) => {
                  setText(event.target.value);
                }}
                data-testid={`cv-question-input-${id}`}
              />
              <Cluster>
                <Button
                  variant="primary"
                  disabled={actions.busy || text.trim() === ""}
                  data-testid={`cv-question-save-${id}`}
                  onClick={() => {
                    actions.edit(id, text.trim());
                    setChanging(false);
                  }}
                >
                  {t("cv.waiting.save")}
                </Button>
                <Button
                  variant="ghost"
                  data-testid={`cv-question-cancel-${id}`}
                  onClick={() => {
                    setChanging(false);
                  }}
                >
                  {t("cv.waiting.cancel")}
                </Button>
              </Cluster>
            </Stack>
          ) : null}
        </Stack>
      </Banner>
    </Tagged>
  );
}

/**
 * "Needs you": every line the app could not fix by itself, as a question
 * with what a yes keeps on the profile; or, when none is left, that every
 * line was checked against the profile and how many the app fixed.
 */
export function WaitingCards({
  waiting,
  fixed,
  actions,
}: {
  waiting: readonly CvWaiting[];
  /** Lines the app rewrote to say only what the profile says. */
  fixed: number;
  actions: LineActions;
}) {
  const { t } = useTranslation("pipeline");
  return (
    <Stack gap="sm">
      <Tagged testId="cv-checked">
        <Text tone="secondary" size="sm">
          {fixed > 0 ? t("cv.waiting.checkedFixed", { count: fixed }) : t("cv.waiting.checked")}
        </Text>
      </Tagged>
      {waiting.length === 0 ? null : (
        <Tagged testId="cv-needs-you">
          <Stack gap="sm">
            <Text weight="semibold">{t("cv.waiting.title", { count: waiting.length })}</Text>
            {waiting.map((item) => (
              <WaitingCard key={item.line_id} item={item} actions={actions} />
            ))}
          </Stack>
        </Tagged>
      )}
    </Stack>
  );
}
