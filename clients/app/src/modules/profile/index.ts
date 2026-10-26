// The profile module (crates/modules/profile): the master profile, imported
// from a LinkedIn export, a PDF or pasted text as a draft to review, and
// edited in place.
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
