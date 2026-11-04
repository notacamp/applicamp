import { useTranslation } from "react-i18next";

import {
  Banner,
  Button,
  Cluster,
  Section,
  Stack,
  Tag,
  Tagged,
  Text,
  type IconName,
} from "~/lib/ui";

import {
  useStartOver,
  type LinkedInFinding,
  type LinkedInProposal,
  type LinkedInReview,
} from "../../api/linkedin";
import { ProposalRow } from "./ProposalRow";
import { TodoList } from "./TodoList";

export interface ReviewStepProps {
  review: LinkedInReview;
  /** Check again with a new file. */
  onAgain: () => void;
}

const ICONS: Record<LinkedInFinding["state"], IconName> = {
  good: "check",
  could_be_better: "edit",
  missing: "warning",
};

/**
 * The review: how the LinkedIn profile reads at a glance, then one block per
 * part with its state, why, what LinkedIn says there now and the proposals,
 * then the list of what to change on LinkedIn. Check again takes a new file;
 * Start over throws the review away.
 */
export function ReviewStep({ review, onAgain }: ReviewStepProps) {
  const { t } = useTranslation("profile");
  const startOver = useStartOver();
  const listed = review.proposals.filter((p) => p.status === "kept" || p.status === "done");
  return (
    <Stack gap="lg" testId="linkedin-review">
      {review.stale ? (
        <Tagged testId="linkedin-stale">
          <Banner
            icon="clock"
            title={t("linkedin.review.staleTitle")}
            action={
              <Button variant="secondary" data-testid="linkedin-stale-again" onClick={onAgain}>
                {t("linkedin.review.again")}
              </Button>
            }
          >
            {t("linkedin.review.stale")}
          </Banner>
        </Tagged>
      ) : null}
      <Text tone="secondary">{t("linkedin.review.lead")}</Text>
      <Section title={t("linkedin.review.overall")} testId="linkedin-overall">
        <Stack gap="sm">
          <Text as="p">{review.overall}</Text>
          <Text tone="tertiary" size="sm">
            {t(`linkedin.review.madeFrom.${review.kind}`)}
          </Text>
        </Stack>
      </Section>
      {review.findings.map((finding) => (
        <FindingBlock
          key={finding.id}
          finding={finding}
          proposals={review.proposals.filter((p) => finding.proposals.includes(p.id))}
        />
      ))}
      <TodoList proposals={listed} />
      <Cluster>
        <Button variant="secondary" data-testid="linkedin-again" onClick={onAgain}>
          {t("linkedin.review.again")}
        </Button>
        <Button
          variant="ghost"
          disabled={startOver.isPending}
          data-testid="linkedin-start-over"
          onClick={() => {
            startOver.mutate();
          }}
        >
          {t("linkedin.review.startOver")}
        </Button>
      </Cluster>
    </Stack>
  );
}

function FindingBlock({
  finding,
  proposals,
}: {
  finding: LinkedInFinding;
  proposals: readonly LinkedInProposal[];
}) {
  const { t } = useTranslation("profile");
  const title =
    finding.part === "position"
      ? (finding.title ?? t("linkedin.review.parts.position"))
      : t(`linkedin.review.parts.${finding.part}`);
  return (
    <Section
      title={title}
      action={
        <Tagged testId={`linkedin-state-${finding.id}`} data={{ "data-state": finding.state }}>
          <Tag
            icon={ICONS[finding.state]}
            tone={finding.state === "missing" ? "danger" : "neutral"}
          >
            {t(`linkedin.review.states.${finding.state}`)}
          </Tag>
        </Tagged>
      }
      testId={`linkedin-finding-${finding.id}`}
    >
      <Stack gap="md">
        <Stack gap="sm">
          <Text as="p">{finding.why}</Text>
          <Text tone="secondary" size="sm">
            {finding.now === null || finding.now === undefined
              ? t("linkedin.review.nothing")
              : t("linkedin.review.now", { text: finding.now })}
          </Text>
        </Stack>
        {proposals.map((proposal) => (
          <ProposalRow key={proposal.id} proposal={proposal} />
        ))}
      </Stack>
    </Section>
  );
}
