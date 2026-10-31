import { JobAnswerFacts } from "~/modules/profile";
import { PreparePage } from "~/modules/pipeline";
import { SpendLine } from "~/modules/sources";

export interface PipelinePrepareProps {
  /** The opportunity (`/pipeline/$id/prepare`). */
  id: string;
  onBack: () => void;
  /** Open the review of the CV (`/pipeline/$id/cv`). */
  onReview: () => void;
}

/**
 * `/pipeline/$id/prepare`: the company's and the job's real facts a tailored
 * CV is based on. An answer's facts are kept, fixed or dropped with Grill
 * me's own controls (the profile module's), on the position they are about.
 */
export function PipelinePrepare({ id, onBack, onReview }: PipelinePrepareProps) {
  return (
    <PreparePage
      id={id}
      onBack={onBack}
      onReview={onReview}
      spend={<SpendLine testId="prepare-month-spend" />}
      renderFacts={(answer, role) => (
        <JobAnswerFacts
          key={answer.question.id}
          position={answer.role.position}
          ids={answer.proposals.map((proposal) => proposal.id)}
          role={role}
          testKey={answer.question.id}
        />
      )}
    />
  );
}
