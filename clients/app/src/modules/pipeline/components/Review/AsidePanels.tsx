import { useTranslation } from "react-i18next";

import { Cluster, Stack, StatusMark, Tagged, Text } from "~/lib/ui";

import type { Cv, CvDraft, CvLine } from "../../api/cv";
import { formatCost } from "../../api/labels";

/** "What the job wants": each want on the CV, or why it is not. */
export function WantsPanel({ draft }: { draft: CvDraft }) {
  const { t } = useTranslation("pipeline");
  if (draft.wants.length === 0) {
    return null;
  }
  return (
    <Tagged testId="cv-wants">
      <Stack gap="sm">
        <Text weight="semibold">{t("cv.wants.title")}</Text>
        {draft.wants.map((want, index) => {
          const parts = [want.text];
          if (want.level === "nice") {
            parts.push(t("cv.wants.nice"));
          }
          if (want.reason === "not_in_profile") {
            parts.push(t("cv.wants.notInProfile"));
          } else if (want.reason === "in_profile") {
            parts.push(t("cv.wants.notOnCv"));
          }
          return (
            <Tagged key={`${want.text}-${String(index)}`} testId={`cv-want-${String(index)}`}>
              <Cluster>
                <StatusMark
                  state={want.covered ? "ok" : "none"}
                  label={want.covered ? t("cv.wants.covered") : t("cv.wants.notCovered")}
                />
                <Text size="sm">{parts.join(" · ")}</Text>
              </Cluster>
            </Tagged>
          );
        })}
      </Stack>
    </Tagged>
  );
}

/** The steps the app's own preparation and draft went through. */
const STEPS = ["shortlist", "site", "read", "compare", "draft"] as const;

/** "Done by the app · €x": what it did by itself for this CV, and what it cost. */
export function DonePanel({ cv }: { cv: Cv }) {
  const { t, i18n } = useTranslation("pipeline");
  const steps = cv.done.filter((step) => (STEPS as readonly string[]).includes(step.step));
  if (steps.length === 0) {
    return null;
  }
  const total = steps.reduce((sum, step) => sum + step.eur, 0);
  const money = (euros: number) => formatCost(euros, i18n.language);
  const rows: { key: string; label: string; eur: number }[] = [
    ...steps.map((step, index) => ({
      key: `${step.step}-${String(index)}`,
      label: t(`cv.done.steps.${step.step as (typeof STEPS)[number]}`),
      eur: step.eur,
    })),
    { key: "checked", label: t("cv.done.steps.checked"), eur: 0 },
    { key: "pdf", label: t("cv.done.steps.pdf"), eur: 0 },
  ];
  return (
    <Tagged testId="cv-done">
      <Stack gap="sm">
        <Text weight="semibold">{t("cv.done.title", { cost: money(total) })}</Text>
        {rows.map((row) => (
          <Tagged key={row.key} testId={`cv-done-${row.key}`}>
            <Cluster>
              <StatusMark state="ok" label={t("cv.done.doneLabel")} />
              <Text size="sm">{t("cv.done.row", { step: row.label, cost: money(row.eur) })}</Text>
            </Cluster>
          </Tagged>
        ))}
      </Stack>
    </Tagged>
  );
}

/** "Fixed by the app": each line it rewrote, with what it said before. */
export function FixedPanel({ draft }: { draft: CvDraft }) {
  const { t } = useTranslation("pipeline");
  const fixed: CvLine[] = [
    draft.headline,
    draft.summary,
    ...draft.experience.flatMap((role) => role.bullets),
  ].filter((line): line is CvLine => line?.fixed_by_app === true);
  if (fixed.length === 0) {
    return null;
  }
  return (
    <Tagged testId="cv-fixed">
      <Stack gap="sm">
        <Text weight="semibold">{t("cv.fixed.title", { count: fixed.length })}</Text>
        {fixed.map((line) => (
          <Tagged key={line.id} testId={`cv-fixed-${line.id}`}>
            <Stack gap="sm">
              <Text size="sm">{t("cv.fixed.now", { text: line.text })}</Text>
              {line.original === undefined || line.original === null ? null : (
                <Text tone="secondary" size="sm">
                  {t("cv.fixed.was", { text: line.original })}
                </Text>
              )}
            </Stack>
          </Tagged>
        ))}
      </Stack>
    </Tagged>
  );
}
