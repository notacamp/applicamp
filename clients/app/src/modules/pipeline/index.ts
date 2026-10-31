// The pipeline module (crates/modules/pipeline): every opportunity, from
// first match to offer, as a table and a board with a side peek; adding a
// job by its link; and Today's state (what to do next).
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
export {
  canPrepare,
  type AnswerProposal,
  type AnswerResult,
  type RoleRef as BriefRole,
} from "./api/brief";
