import { useTranslation } from "react-i18next";

import { Page } from "~/lib/ui";
import { CvsPage } from "~/modules/pipeline";

export interface CvsProps {
  /** Review a draft's CV (`/pipeline/$id/cv`). */
  onReview: (id: string) => void;
  /** Open the job in the pipeline. */
  onOpportunity: (id: string) => void;
  onPipeline: () => void;
}

/** `/cvs`: every CV, as PDFs to view and drafts to review, in the app frame. */
export function Cvs({ onReview, onOpportunity, onPipeline }: CvsProps) {
  const { t } = useTranslation();
  return (
    <Page testId="cvs" title={t("cvs.title")} lead={t("cvs.lead")} wide>
      <CvsPage onReview={onReview} onOpportunity={onOpportunity} onPipeline={onPipeline} />
    </Page>
  );
}
