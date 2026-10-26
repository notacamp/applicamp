import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import {
  Banner,
  Bullets,
  Button,
  ButtonLink,
  Cluster,
  Heading,
  InlineText,
  Label,
  MatchBar,
  Property,
  PropertyList,
  Section,
  Select,
  SidePeek,
  Tagged,
  Text,
  useUndoToast,
} from "~/lib/ui";

import { formatShortDate, SOURCE_HUES } from "../../api/labels";
import {
  STAGES,
  useOpportunity,
  useUpdateOpportunity,
  type Opportunity,
  type OpportunityChanges,
} from "../../api/opportunities";
import { titleOf } from "../PipelinePage/OpportunityViews";

/** How much of the posting's text the peek shows. */
export const EXCERPT_CHARS = 1500;

export interface OpportunityPeekProps {
  /** The opportunity open; none closes the peek. */
  id: string | null;
  onClose: () => void;
}

/**
 * One opportunity beside the list: its properties (stage, next step), why
 * it matches and its gaps, the posting's text and link, notes, and Not for
 * me. Opening it once marks a new match seen.
 */
export function OpportunityPeek({ id, onClose }: OpportunityPeekProps) {
  const { t } = useTranslation("pipeline");
  const opportunity = useOpportunity(id);
  const data = id === null ? undefined : opportunity.data;
  const label = data === undefined ? t("peek.eyebrow") : titleOf(data, t("untitled"));
  return (
    <SidePeek
      open={id !== null}
      label={label}
      eyebrow={t("peek.eyebrow")}
      closeLabel={t("peek.close")}
      onClose={onClose}
      testId="peek"
      closeTestId="peek-close"
    >
      {opportunity.isError ? (
        <Tagged testId="peek-error">
          <Banner tone="danger" icon="warning" title={t("loadFailed")} />
        </Tagged>
      ) : null}
      {data === undefined && !opportunity.isError ? (
        <Text tone="secondary">{t("loading")}</Text>
      ) : null}
      {data === undefined ? null : <PeekBody key={data.id} opportunity={data} onClose={onClose} />}
    </SidePeek>
  );
}

function PeekBody({ opportunity, onClose }: { opportunity: Opportunity; onClose: () => void }) {
  const { t, i18n } = useTranslation("pipeline");
  const update = useUpdateOpportunity();
  const undo = useUndoToast();
  const save = (changes: OpportunityChanges) => {
    update.mutate({ id: opportunity.id, changes });
  };

  // A new match is seen once it is opened.
  const marked = useRef(false);
  useEffect(() => {
    if (opportunity.new && !marked.current) {
      marked.current = true;
      update.mutate({ id: opportunity.id, changes: { opened: true } });
    }
  }, [opportunity.id, opportunity.new, update]);

  const title = titleOf(opportunity, t("untitled"));
  const notForMe = () => {
    update.mutate(
      { id: opportunity.id, changes: { dismissed: true } },
      {
        onSuccess: () => {
          undo.push({
            label: t("peek.dismissed", { title }),
            undo: () => {
              update.mutate({ id: opportunity.id, changes: { dismissed: false } });
            },
          });
          onClose();
        },
      },
    );
  };
  const editable = opportunity.editable;
  const description = opportunity.description ?? "";
  const excerpt =
    description.length > EXCERPT_CHARS ? `${description.slice(0, EXCERPT_CHARS)}…` : description;
  const textProps = {
    saveLabel: t("peek.save"),
    cancelLabel: t("peek.cancel"),
    busy: update.isPending,
  };

  return (
    <>
      {editable ? (
        <InlineText
          {...textProps}
          label={t("peek.role")}
          value={opportunity.title}
          placeholder={t("peek.addRole")}
          onSave={(value) => {
            save({ title: value });
          }}
          testId="peek-title"
        />
      ) : (
        <Heading level={2} size="page">
          {title}
        </Heading>
      )}
      <PropertyList testId="peek-properties">
        <Property label={t("peek.company")} testId="peek-company">
          {editable ? (
            <InlineText
              {...textProps}
              label={t("peek.company")}
              value={opportunity.company}
              placeholder={t("peek.addCompany")}
              onSave={(value) => {
                save({ company: value });
              }}
              testId="peek-company-edit"
            />
          ) : (
            opportunity.company
          )}
        </Property>
        <Property label={t("peek.where")} testId="peek-where">
          {editable ? (
            <InlineText
              {...textProps}
              label={t("peek.where")}
              value={opportunity.location ?? undefined}
              placeholder={t("peek.addWhere")}
              onSave={(value) => {
                save({ location: value === "" ? null : value });
              }}
              testId="peek-where-edit"
            />
          ) : (
            [
              opportunity.location ?? null,
              opportunity.work_mode === undefined || opportunity.work_mode === null
                ? null
                : t(`workModes.${opportunity.work_mode}`),
            ]
              .filter((part) => part !== null && part !== "")
              .join(" · ") || t("peek.unknown")
          )}
        </Property>
        {opportunity.salary_text === undefined || opportunity.salary_text === null ? null : (
          <Property label={t("peek.pay")}>{opportunity.salary_text}</Property>
        )}
        <Property label={t("peek.match")}>
          <MatchBar
            value={opportunity.match}
            label={t("columns.match")}
            empty={t("peek.notScored")}
          />
        </Property>
        <Property label={t("peek.stage")}>
          <Select
            label={t("peek.stage")}
            hideLabel
            options={STAGES.map((stage) => ({ value: stage, label: t(`stages.${stage}`) }))}
            value={opportunity.stage}
            onChange={(stage) => {
              save({ stage });
            }}
            data-testid="peek-stage"
          />
        </Property>
        <Property label={t("peek.source")}>
          <Label hue={SOURCE_HUES[opportunity.source]}>{t(`sources.${opportunity.source}`)}</Label>
        </Property>
        <Property label={t("peek.nextStep")}>
          <InlineText
            {...textProps}
            label={t("peek.nextStep")}
            value={opportunity.next_step ?? undefined}
            placeholder={t("peek.addNextStep")}
            onSave={(value) => {
              save({ next_step: value === "" ? null : value });
            }}
            testId="peek-next-step"
          />
        </Property>
        {opportunity.posted_at === undefined || opportunity.posted_at === null ? null : (
          <Property label={t("peek.posted")}>
            {formatShortDate(opportunity.posted_at, i18n.language)}
          </Property>
        )}
      </PropertyList>
      {update.isError ? (
        <Tagged testId="peek-save-error">
          <Banner tone="danger" icon="warning" role="alert" title={t("peek.saveFailed")} />
        </Tagged>
      ) : null}
      {opportunity.why.length === 0 ? null : (
        <Section title={t("peek.why")} testId="peek-why">
          <Bullets items={opportunity.why} />
        </Section>
      )}
      {opportunity.gaps.length === 0 ? null : (
        <Section title={t("peek.gaps")} testId="peek-gaps">
          <Bullets items={opportunity.gaps} />
        </Section>
      )}
      <Section title={t("peek.notes")} testId="peek-notes">
        <InlineText
          {...textProps}
          multiline
          maxLength={5000}
          counter={(left) => t("peek.charactersLeft", { left: String(left) })}
          label={t("peek.notes")}
          value={opportunity.notes ?? undefined}
          placeholder={t("peek.addNotes")}
          onSave={(value) => {
            save({ notes: value === "" ? null : value });
          }}
          testId="peek-notes-edit"
        />
      </Section>
      {excerpt === "" ? null : (
        <Section title={t("peek.description")} testId="peek-description">
          <Text tone="secondary" as="div">
            {excerpt}
          </Text>
        </Section>
      )}
      <Cluster>
        <ButtonLink
          variant="secondary"
          icon="forward"
          href={opportunity.url}
          target="_blank"
          rel="noopener noreferrer"
          data-testid="peek-open-posting"
        >
          {t("peek.open")}
        </ButtonLink>
        <Button
          variant="ghost"
          icon="close"
          disabled={update.isPending}
          data-testid="peek-not-for-me"
          onClick={notForMe}
        >
          {t("peek.notForMe")}
        </Button>
      </Cluster>
    </>
  );
}
