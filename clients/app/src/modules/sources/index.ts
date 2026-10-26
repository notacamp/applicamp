// The sources module (crates/modules/sources): searching the public job
// boards against your goals, its progress, and what the AI it uses costs.
export {
  formatEuros,
  isRunning,
  latestSearchQuery,
  useLatestSearch,
  type Search,
  type Spend,
} from "./api/searches";
export { SearchNowButton, type SearchNowButtonProps } from "./components/SearchNow/SearchNowButton";
export {
  SearchLine,
  type SearchLineProps,
  type SearchSummary,
} from "./components/SearchLine/SearchLine";
export { SearchProgress, type SearchProgressProps } from "./components/SearchLine/SearchProgress";
export { SpendLine } from "./components/SpendLine/SpendLine";
