import { LetterPage } from "~/modules/pipeline";
import { SpendLine } from "~/modules/sources";

export interface PipelineLetterProps {
  /** The opportunity (`/pipeline/$id/letter`). */
  id: string;
  /** Back to the CV. */
  onBack: () => void;
  /** Back to the pipeline. */
  onPipeline: () => void;
}

/**
 * `/pipeline/$id/letter`: the review of the cover letter, in the app frame,
 * with the month's AI spend line (the sources module's) beside the draft's own.
 */
export function PipelineLetter({ id, onBack, onPipeline }: PipelineLetterProps) {
  return (
    <LetterPage
      id={id}
      onBack={onBack}
      onPipeline={onPipeline}
      spend={<SpendLine testId="letter-month-spend" />}
    />
  );
}
