import { ReviewPage } from "~/modules/pipeline";
import { SpendLine } from "~/modules/sources";

export interface PipelineCvProps {
  /** The opportunity (`/pipeline/$id/cv`). */
  id: string;
  /** Back to Prepare. */
  onBack: () => void;
  /** Back to the pipeline. */
  onPipeline: () => void;
}

/**
 * `/pipeline/$id/cv`: the review of the tailored CV, in the app frame, with
 * the month's AI spend line (the sources module's) beside the draft's own.
 */
export function PipelineCv({ id, onBack, onPipeline }: PipelineCvProps) {
  return (
    <ReviewPage
      id={id}
      onBack={onBack}
      onPipeline={onPipeline}
      spend={<SpendLine testId="cv-month-spend" />}
    />
  );
}
