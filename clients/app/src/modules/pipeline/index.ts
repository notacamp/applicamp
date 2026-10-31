// The pipeline module (crates/modules/pipeline): every opportunity, from
// first match to offer, as a table and a board with a side peek; adding a
// job by its link; Today's state (what to do next); the Prepare page (the real
// facts a tailored CV is based on); and the review of the CV (every line traced
// to the profile, approved only when none is blocked).
export {
  invalidatePipeline,
  STAGES,
  type Opportunity,
  type OpportunityStage,
  type OpportunitySummary,
} from "./api/opportunities";
export { todayQuery, useToday, type NextStep, type Today, type TodaySearch } from "./api/today";
export { PipelinePage, type PipelinePageProps } from "./components/PipelinePage/PipelinePage";
export { PreparePage, type PreparePageProps } from "./components/Prepare/PreparePage";
export { ReviewPage, type ReviewPageProps } from "./components/Review/ReviewPage";
export {
  canPrepare,
  type AnswerProposal,
  type AnswerResult,
  type RoleRef as BriefRole,
} from "./api/brief";
