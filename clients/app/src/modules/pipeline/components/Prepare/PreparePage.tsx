import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import { Banner, Button, Cluster, Label, Page, Section, Stack, Tagged, Text } from "~/lib/ui";

import { useBrief, type AnswerResult, type Brief } from "../../api/brief";
import { formatCost } from "../../api/labels";
import { useOpportunity } from "../../api/opportunities";
import { titleOf } from "../PipelinePage/OpportunityViews";
import { CompanyFacts } from "./CompanyFacts";
import { Questions } from "./Questions";

export interface PreparePageProps {
  /** The opportunity. */
  id: string;
  /** Back to the pipeline. */
  onBack: () => void;
  /**
   * The facts taken from an answer, each with Keep, Fix and Drop: Grill me's,
   * from the profile module (modules never import each other's parts).
   */
  renderFacts: (answer: AnswerResult, role: string) => ReactNode;
  /** The month's AI spend line. */
  spend?: ReactNode;
  /** Where "Draft the CV" arrives (the next change). */
  next?: ReactNode;
}

/**
 * `/pipeline/$id/prepare`: what a tailored CV for one opportunity is based
 * on, all of it real: facts read on the company's own site (each with its
 * page), what the job wants most (each with the posting's words), and the
 * few questions where the profile is thin. Opening it asks nothing of the
 * model and fetches nothing; every step starts with a click.
 */
export function PreparePage({ id, onBack, renderFacts, spend, next }: PreparePageProps) {
  const { t } = useTranslation("pipeline");
  const opportunity = useOpportunity(id);
  const brief = useBrief(id);
  const title =
    opportunity.data === undefined ? t("prepare.title") : titleOf(opportunity.data, t("untitled"));
  const company = opportunity.data?.company ?? "";

  let body: ReactNode;
  if (brief.isPending || opportunity.isPending) {
    body = (
      <Tagged testId="prepare-loading">
        <Text tone="secondary">{t("prepare.loading")}</Text>
      </Tagged>
    );
  } else if (brief.isError || opportunity.isError) {
    const missing =
      problemCode(brief.error) === "not_found" || problemCode(opportunity.error) === "not_found";
    body = (
      <Tagged testId="prepare-load-error">
        <Banner
          tone="danger"
          icon="warning"
          title={t(missing ? "prepare.errors.not_found" : "prepare.loadFailed")}
          action={
            missing ? undefined : (
              <Button
                variant="secondary"
                onClick={() => {
                  void brief.refetch();
                  void opportunity.refetch();
                }}
              >
                {t("retry")}
              </Button>
            )
          }
        />
      </Tagged>
    );
  } else {
    body = (
      <PrepareBody id={id} brief={brief.data} renderFacts={renderFacts} spend={spend} next={next} />
    );
  }

  return (
    <Page
      testId="prepare"
      eyebrow={t("prepare.eyebrow")}
      title={company === "" ? title : t("prepare.heading", { title, company })}
      lead={t("prepare.lead")}
    >
      <Cluster>
        <Button variant="ghost" data-testid="prepare-back" onClick={onBack}>
          {t("prepare.back")}
        </Button>
      </Cluster>
      {body}
    </Page>
  );
}

function PrepareBody({
  id,
  brief,
  renderFacts,
  spend,
  next,
}: {
  id: string;
  brief: Brief;
  renderFacts: PreparePageProps["renderFacts"];
  spend: ReactNode;
  next: ReactNode;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const language = i18n.language;
  const company = brief.company === "" ? t("prepare.thisCompany") : brief.company;
  const open = brief.questions.filter((question) => question.state === "open").length;
  const wants = brief.wants ?? undefined;

  if (brief.company.trim() === "") {
    return (
      <Tagged testId="prepare-no-company">
        <Banner icon="warning" title={t("prepare.errors.company_unknown")} />
      </Tagged>
    );
  }
  return (
    <Stack gap="lg">
      <Section title={t("prepare.companyTitle", { company })} testId="prepare-company">
        <CompanyFacts id={id} brief={brief} />
      </Section>

      <Section title={t("prepare.wants.title")} testId="prepare-wants-section">
        {wants === undefined ? (
          <Tagged testId="prepare-wants-pending">
            <Text tone="secondary">
              {brief.read === undefined || brief.read === null
                ? t("prepare.wants.pending")
                : t("prepare.wants.noText")}
            </Text>
          </Tagged>
        ) : wants.length === 0 ? (
          <Tagged testId="prepare-wants-none">
            <Text tone="secondary">{t("prepare.wants.none")}</Text>
          </Tagged>
        ) : (
          <Cluster>
            {wants.map((want, index) => (
              <Tagged
                key={`${want.label}-${String(index)}`}
                testId={`prepare-want-${String(index)}`}
              >
                <Label hue={want.level === "must" ? "blue" : "gray"}>
                  {want.level === "must"
                    ? want.label
                    : t("prepare.wants.nice", { label: want.label })}
                </Label>
              </Tagged>
            ))}
          </Cluster>
        )}
      </Section>

      {wants === undefined && brief.questions.length === 0 ? null : (
        <Section
          title={
            open === 0
              ? t("prepare.questions.titleNone")
              : t("prepare.questions.title", { count: open })
          }
          description={t("prepare.questions.lead")}
          testId="prepare-questions"
        >
          <Questions id={id} brief={brief} renderFacts={renderFacts} />
        </Section>
      )}

      <Section title={t("prepare.draft.title")} testId="prepare-draft-slot">
        <Text tone="secondary">{t("prepare.draft.lead")}</Text>
        {next}
      </Section>

      <Stack gap="sm">
        <Tagged testId="prepare-cost">
          <Text tone="secondary" size="sm">
            {t("prepare.costLine", {
              read: formatCost(brief.estimates.company_read_eur, language),
              analysis: formatCost(brief.estimates.analysis_eur, language),
              answer: formatCost(brief.estimates.answer_eur, language),
            })}
          </Text>
        </Tagged>
        <Tagged testId="prepare-spend">
          <Text tone="secondary" size="sm">
            {spendLine(brief, t)}
          </Text>
        </Tagged>
        {spend}
      </Stack>
    </Stack>
  );
}

function spendLine(brief: Brief, t: ReturnType<typeof useTranslation<"pipeline">>["t"]): string {
  // The month's spend is the shared spend line beside it; this says only the reads.
  return t("prepare.spend.reads", { n: brief.spend.reads_today, cap: brief.spend.reads_cap });
}
