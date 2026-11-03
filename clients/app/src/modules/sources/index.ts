// The sources module (crates/modules/sources): searching the job boards
// against your goals, its progress, and what the AI it uses costs; the
// Boards page (approved boards ranked by salary, the ones waiting for the
// owner, and what each search scanned).
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
export {
  BoardsPage,
  type BoardsPageProps,
  type BoardsTab,
} from "./components/BoardsPage/BoardsPage";
// Company watchers (#20): companies whose jobs keep fitting you, suggested
// from your matches, whose own job board is read every day; Today's
// compact suggestion.
export {
  COMPANIES_QUERY_KEY,
  useCompanies,
  type CompanyList,
  type CompanySuggestion,
  type WatchedCompany,
} from "./api/companies";
export {
  CompaniesPage,
  type CompaniesPageProps,
  type CompaniesTab,
} from "./components/CompaniesPage/CompaniesPage";
export {
  TodaySuggestion,
  type TodaySuggestionProps,
} from "./components/TodaySuggestion/TodaySuggestion";
