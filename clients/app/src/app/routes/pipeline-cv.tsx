import { ReviewPage } from "~/modules/pipeline";
import { SpendLine } from "~/modules/sources";

export interface PipelineCvProps {
  /** The opportunity (`/pipeline/$id/cv`). */
  id: string;
  /** Back to Prepare. */
  onBack: () => void;
  /** Back to the pipeline. */
  onPipeline: () => void;
  /** Open the cover letter's review. */
  onLetter: (id: string) => void;
}

/**
 * `/pipeline/$id/cv`: the review of the tailored CV, in the app frame, with
 * the month's AI spend line (the sources module's) beside the draft's own.
 */
export function PipelineCv({ id, onBack, onPipeline, onLetter }: PipelineCvProps) {
  return (
    <ReviewPage
      id={id}
      onBack={onBack}
      onPipeline={onPipeline}
      onLetter={onLetter}
      spend={<SpendLine testId="cv-month-spend" />}
    />
  );
}
