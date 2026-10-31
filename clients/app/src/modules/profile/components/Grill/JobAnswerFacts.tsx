import { useTranslation } from "react-i18next";

import { Tagged, Text } from "~/lib/ui";

import { useGrillRole } from "../../api/grill";
import { FactsTaken } from "./FactsTaken";

export interface JobAnswerFactsProps {
  /** The position the facts wait on (its place in the profile's positions). */
  position: number;
  /** The facts an answer about a job yielded (their proposal ids). */
  ids: readonly string[];
  /** The position's name, for "Saved to …". */
  role: string;
  /** Tells this block's test ids from another answer's. */
  testKey: string;
}

/**
 * Facts taken from an answer about a job (the tailored CV's questions), with
 * Grill me's own Keep, Fix and Drop: the same checks, the same buttons. They
 * wait on the position for 24 hours and reach the profile only on a click.
 */
export function JobAnswerFacts({ position, ids, role, testKey }: JobAnswerFactsProps) {
  const { t } = useTranslation("profile");
  const view = useGrillRole(position);
  if (view.data === undefined) {
    return (
      <Tagged testId={`job-answer-loading-${testKey}`}>
        <Text tone="secondary">{t("grill.loading")}</Text>
      </Tagged>
    );
  }
  const facts = view.data.proposals.filter((proposal) => ids.includes(proposal.id));
  return <FactsTaken view={view.data} facts={facts} turn={testKey} role={role} />;
}
