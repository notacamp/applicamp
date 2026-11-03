import { useTranslation } from "react-i18next";

import { Button, CompanyActions, CompanyCard, Tagged, Text } from "~/lib/ui";

import { useUpdateOpportunity } from "../../api/opportunities";
import type { CompanyHit } from "../../api/today";

export interface CompanyHitCardProps {
  hit: CompanyHit;
  /** Open the job in the Pipeline's peek. */
  onOpen: (id: string) => void;
}

/**
 * "Northwind Stays posted a job that fits you": found on the own job board of a
 * company the person watches, and already in the Pipeline as Found. Open it, or
 * say it is not for them.
 */
export function CompanyHitCard({ hit, onOpen }: CompanyHitCardProps) {
  const { t, i18n } = useTranslation("pipeline");
  const update = useUpdateOpportunity();
  const at = new Date(hit.found_at);
  const today = at.toDateString() === new Date().toDateString();
  const mode =
    hit.work_mode === "remote" || hit.work_mode === "hybrid" || hit.work_mode === "onsite"
      ? t(`workModes.${hit.work_mode}`)
      : null;
  const line = [hit.title, hit.location ?? mode, hit.salary_text]
    .filter((part): part is string => part !== null && part !== undefined && part !== "")
    .join(" · ");
  const found = today
    ? t("hit.foundToday", {
        time: new Intl.DateTimeFormat(i18n.language, {
          hour: "2-digit",
          minute: "2-digit",
        }).format(at),
      })
    : t("hit.foundOn", {
        date: new Intl.DateTimeFormat(i18n.language, { day: "numeric", month: "short" }).format(at),
      });
  const fit =
    hit.match_score === undefined || hit.match_score === null
      ? null
      : t("hit.fits", { score: String(hit.match_score) });
  return (
    <CompanyCard title={t("hit.title", { company: hit.company })} testId={`today-hit-${hit.id}`}>
      <Text>{line}</Text>
      <Tagged testId={`today-hit-found-${hit.id}`}>
        <Text tone="secondary" size="sm">
          {[found, fit, t("hit.inPipeline")].filter((part) => part !== null).join(" · ")}
        </Text>
      </Tagged>
      <CompanyActions>
        <Button
          variant="primary"
          onClick={() => {
            onOpen(hit.id);
          }}
          data-testid={`today-hit-open-${hit.id}`}
        >
          {t("hit.open")}
        </Button>
        <Button
          variant="secondary"
          disabled={update.isPending}
          onClick={() => {
            update.mutate({ id: hit.id, changes: { dismissed: true } });
          }}
          data-testid={`today-hit-dismiss-${hit.id}`}
        >
          {t("hit.notForMe")}
        </Button>
      </CompanyActions>
    </CompanyCard>
  );
}
