import { useEffect, useRef, useState } from "react";
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
  PdfViewer,
  PropertyList,
  Section,
  Select,
  SidePeek,
  Stack,
  Tagged,
  Text,
  useUndoToast,
} from "~/lib/ui";

import { canPrepare, PREPARE_ESTIMATE_EUR } from "../../api/brief";
import { cvPdfUrl, useCv } from "../../api/cv";
import { letterPdfUrl, useLetter } from "../../api/letter";
import {
  factsText,
  gapText,
  formatCost,
  formatShortDate,
  hiddenReasonText,
  placeText,
  sourceHue,
  sourceLabel,
} from "../../api/labels";
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
  /** Open the page that prepares a tailored CV for it; none hides the Tailored CV block. */
  onPrepare?: ((id: string) => void) | undefined;
  /** Open the review of its tailored CV. */
  onReview?: ((id: string) => void) | undefined;
  /** Open the review of its cover letter; none hides the Cover letter line. */
  onLetter?: ((id: string) => void) | undefined;
}

/**
 * One opportunity beside the list: its properties (stage, next step, what
 * the posting says about the person's requirements), why it matches and its
 * gaps, the posting's text and link, notes, and Not for me. One that doesn't
 * meet the requirements says so. Opening it once marks a new match seen.
 */
export function OpportunityPeek({
  id,
  onClose,
  onPrepare,
  onReview,
  onLetter,
}: OpportunityPeekProps) {
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
      {data === undefined ? null : (
        <PeekBody
          key={data.id}
          opportunity={data}
          onClose={onClose}
          onPrepare={onPrepare}
          onReview={onReview}
          onLetter={onLetter}
        />
      )}
    </SidePeek>
  );
}

/** The Tailored CV block: where the CV is (none yet, a draft, a version), and the way on. */
function TailoredCv({
  opportunity,
  onPrepare,
  onReview,
  onLetter,
}: {
  opportunity: Opportunity;
  onPrepare: (id: string) => void;
  onReview: (id: string) => void;
  onLetter: ((id: string) => void) | undefined;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const cv = useCv(opportunity.id);
  const draft = cv.data?.draft ?? undefined;
  const approved = cv.data?.versions[0];
  const [viewing, setViewing] = useState(false);
  const state =
    draft === undefined
      ? t("peek.tailored.none")
      : draft.status === "approved"
        ? t("peek.tailored.approved", { version: draft.version ?? 1 })
        : draft.flagged > 0
          ? t("peek.tailored.needsYou", { n: draft.n, count: draft.flagged })
          : t("peek.tailored.clean", { n: draft.n });
  return (
    <Section
      title={t("peek.tailored.title")}
      description={t("peek.tailored.lead")}
      testId="peek-tailored"
    >
      {cv.data === undefined ? null : (
        <Tagged testId="peek-cv-state">
          <Text>{state}</Text>
        </Tagged>
      )}
      {approved === undefined ? null : (
        <Cluster>
          <Button
            variant="secondary"
            data-testid="peek-view-pdf"
            onClick={() => {
              setViewing(true);
            }}
          >
            {t("peek.tailored.viewPdf")}
          </Button>
          <ButtonLink
            variant="secondary"
            icon="download"
            href={cvPdfUrl(opportunity.id, approved.version, true)}
            data-testid="peek-download-pdf"
          >
            {t("peek.tailored.download")}
          </ButtonLink>
        </Cluster>
      )}
      {approved === undefined || draft === undefined || draft.status === "approved" ? null : (
        <Tagged testId="peek-cv-approved">
          <Text tone="secondary" size="sm">
            {t("peek.tailored.approvedBefore", { version: approved.version })}
          </Text>
        </Tagged>
      )}
      <Cluster>
        {draft === undefined ? (
          <Button
            variant="primary"
            icon="arrow-right"
            data-testid="peek-prepare"
            onClick={() => {
              onPrepare(opportunity.id);
            }}
          >
            {t("peek.tailored.prepare")}
          </Button>
        ) : (
          <Button
            variant="primary"
            icon="arrow-right"
            data-testid="peek-review"
            onClick={() => {
              onReview(opportunity.id);
            }}
          >
            {t("peek.tailored.review")}
          </Button>
        )}
        {draft === undefined ? (
          <Text tone="secondary" size="sm">
            {t("peek.tailored.cost", { cost: formatCost(PREPARE_ESTIMATE_EUR, i18n.language) })}
          </Text>
        ) : null}
      </Cluster>
      {onLetter === undefined || approved === undefined ? null : (
        <CoverLetter opportunity={opportunity} onLetter={onLetter} />
      )}
      <PdfViewer
        open={viewing && approved !== undefined}
        onClose={() => {
          setViewing(false);
        }}
        url={cvPdfUrl(opportunity.id, approved?.version ?? 0)}
        downloadUrl={cvPdfUrl(opportunity.id, approved?.version ?? 0, true)}
        title={t("cvs.viewer.title", {
          company: opportunity.company,
          version: approved?.version ?? 0,
        })}
        labels={{
          close: t("cv.viewer.close"),
          download: t("cv.pdf.download"),
          loading: t("cv.viewer.loading"),
          failed: t("cv.viewer.failed"),
          page: (page, pages) => t("cv.viewer.page", { page, pages }),
        }}
      />
    </Section>
  );
}

/** The Cover letter line of the Tailored CV block: draft or approved, with Review and View PDF. */
function CoverLetter({
  opportunity,
  onLetter,
}: {
  opportunity: Opportunity;
  onLetter: (id: string) => void;
}) {
  const { t } = useTranslation("pipeline");
  const letter = useLetter(opportunity.id);
  const [viewing, setViewing] = useState(false);
  if (letter.data === undefined) {
    return null;
  }
  const draft = letter.data.draft ?? undefined;
  const approved = letter.data.versions[0];
  const state =
    draft === undefined
      ? t("peek.letter.none")
      : draft.status === "approved"
        ? t("peek.letter.approved", { version: draft.version ?? 1 })
        : draft.flagged > 0
          ? t("peek.letter.needsYou", { n: draft.n, count: draft.flagged })
          : t("peek.letter.clean", { n: draft.n });
  return (
    <Tagged testId="peek-letter">
      <Stack gap="sm">
        <Text weight="semibold">{t("peek.letter.title")}</Text>
        <Tagged testId="peek-letter-state">
          <Text>{state}</Text>
        </Tagged>
        <Cluster>
          <Button
            variant={draft === undefined ? "primary" : "secondary"}
            icon="arrow-right"
            data-testid="peek-letter-review"
            onClick={() => {
              onLetter(opportunity.id);
            }}
          >
            {draft === undefined ? t("peek.letter.write") : t("peek.letter.review")}
          </Button>
          {approved === undefined ? null : (
            <>
              <Button
                variant="secondary"
                data-testid="peek-letter-view-pdf"
                onClick={() => {
                  setViewing(true);
                }}
              >
                {t("peek.letter.viewPdf")}
              </Button>
              <ButtonLink
                variant="secondary"
                icon="download"
                href={letterPdfUrl(opportunity.id, approved.version, true)}
                data-testid="peek-letter-download-pdf"
              >
                {t("peek.letter.download")}
              </ButtonLink>
            </>
          )}
        </Cluster>
        <PdfViewer
          open={viewing && approved !== undefined}
          onClose={() => {
            setViewing(false);
          }}
          url={letterPdfUrl(opportunity.id, approved?.version ?? 0)}
          downloadUrl={letterPdfUrl(opportunity.id, approved?.version ?? 0, true)}
          title={t("letter.viewerCompany", {
            company: opportunity.company,
            version: approved?.version ?? 0,
          })}
          labels={{
            close: t("cv.viewer.close"),
            download: t("cv.pdf.download"),
            loading: t("cv.viewer.loading"),
            failed: t("cv.viewer.failed"),
            page: (page, pages) => t("cv.viewer.page", { page, pages }),
          }}
        />
      </Stack>
    </Tagged>
  );
}

function PeekBody({
  opportunity,
  onClose,
  onPrepare,
  onReview,
  onLetter,
}: {
  opportunity: Opportunity;
  onClose: () => void;
  onPrepare: ((id: string) => void) | undefined;
  onReview: ((id: string) => void) | undefined;
  onLetter: ((id: string) => void) | undefined;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const update = useUpdateOpportunity(i18n.language);
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

  const facts = opportunity.facts ?? null;
  // What it asks of where the person is, beyond the region (#111).
  const timeZones = facts?.time_zone_note?.trim() ?? "";
  const workPermit = facts?.work_permit_note?.trim() ?? "";
  const relocation = facts?.relocation_note?.trim() ?? "";
  const hiddenReason = opportunity.hidden_reason ?? null;

  return (
    <>
      {hiddenReason === null ? null : (
        <Tagged testId="peek-hidden">
          <Banner icon="warning" title={t("peek.hidden")}>
            {hiddenReasonText(hiddenReason, facts?.region_note, t)}
          </Banner>
        </Tagged>
      )}
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
            placeText(opportunity, t) || t("peek.unknown")
          )}
        </Property>
        {opportunity.salary_text === undefined || opportunity.salary_text === null ? null : (
          <Property label={t("peek.pay")}>{opportunity.salary_text}</Property>
        )}
        {facts === null ? null : (
          <Property label={t("peek.requirements")} testId="peek-facts">
            {factsText(facts, i18n.language, t)}
          </Property>
        )}
        {timeZones === "" ? null : (
          <Property label={t("peek.timeZones")} testId="peek-time-zones">
            {timeZones}
          </Property>
        )}
        {workPermit === "" ? null : (
          <Property label={t("peek.workPermit")} testId="peek-work-permit">
            {workPermit}
          </Property>
        )}
        {relocation === "" ? null : (
          <Property label={t("peek.relocation")} testId="peek-relocation">
            {relocation}
          </Property>
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
          <Label hue={sourceHue(opportunity.source)}>{sourceLabel(opportunity, t)}</Label>
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
      {opportunity.reasons_updated ? (
        <Tagged testId="peek-reasons-updated">
          <Text tone="secondary">{t("peek.reasonsUpdated")}</Text>
        </Tagged>
      ) : null}
      {opportunity.gaps.length === 0 ? null : (
        <Section title={t("peek.gaps")} testId="peek-gaps">
          <Bullets items={opportunity.gaps.map((gap) => gapText(gap, i18n.language, t))} />
        </Section>
      )}
      {onPrepare === undefined || !canPrepare(opportunity.stage) ? null : (
        <TailoredCv
          opportunity={opportunity}
          onPrepare={onPrepare}
          onReview={onReview ?? onPrepare}
          onLetter={onLetter}
        />
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
      {opportunity.sources.length === 0 ? null : (
        <Section title={t("peek.sources")} testId="peek-sources">
          <Cluster>
            {opportunity.sources.map((source) => (
              // A followed link to the posting on the board, as its terms ask: no `nofollow`.
              <ButtonLink
                key={source.board}
                variant="ghost"
                icon="forward"
                href={source.url}
                target="_blank"
                rel="noopener noreferrer"
                data-testid="peek-source-link"
              >
                {t("peek.from", { name: source.name })}
              </ButtonLink>
            ))}
          </Cluster>
        </Section>
      )}
      <Cluster>
        {opportunity.apply_url === undefined || opportunity.apply_url === null ? null : (
          // The board's apply link, exactly as it gives it.
          <ButtonLink
            variant="primary"
            icon="forward"
            href={opportunity.apply_url}
            target="_blank"
            rel="noopener noreferrer"
            data-testid="peek-apply"
          >
            {t("peek.apply")}
          </ButtonLink>
        )}
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
