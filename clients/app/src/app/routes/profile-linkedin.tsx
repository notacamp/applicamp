import { LinkedInPage } from "~/modules/profile";

export interface ProfileLinkedInProps {
  /** Back to Profile & goals. */
  onBack: () => void;
}

/**
 * `/profile/linkedin`: "Your LinkedIn", in the app frame. The LinkedIn
 * profile from a file the person uploads, reviewed against their profile
 * and goals (one model call), with what to change on LinkedIn to keep or
 * drop.
 */
export function ProfileLinkedIn({ onBack }: ProfileLinkedInProps) {
  return <LinkedInPage onBack={onBack} />;
}
