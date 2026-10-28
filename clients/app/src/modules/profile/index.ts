// The profile module (crates/modules/profile): the master profile, imported
// from a LinkedIn export, a PDF or pasted text as a draft to review, and
// edited in place; and goals, asked as a short interview one question at a time.
export {
  isEmptyProfile,
  profileQuery,
  useProfile,
  type Profile,
  type ProfileView,
} from "./api/profile";
export {
  ProfileWorkspace,
  type ProfileWorkspaceProps,
} from "./components/ProfilePage/ProfileWorkspace";
export { useGoals, type Goals, type GoalsView } from "./api/goals";
export { QUESTIONS, type Question } from "./api/questions";
export { GoalInterview, type GoalInterviewProps } from "./components/GoalInterview/GoalInterview";
export { parseAction, type Action, type StrengthItem, type StrengthView } from "./api/completion";
export {
  ImproveProfile,
  type ImproveProfileProps,
} from "./components/ImproveProfile/ImproveProfile";
export { SkillsPanel, type SkillsPanelProps } from "./components/SkillsPanel/SkillsPanel";
export {
  StrengthNudge,
  type NudgeAction,
  type StrengthNudgeProps,
} from "./components/StrengthNudge/StrengthNudge";
