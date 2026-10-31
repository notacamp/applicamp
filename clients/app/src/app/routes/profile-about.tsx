import { AboutYouPage } from "~/modules/profile";
import { SpendLine } from "~/modules/sources";

export interface ProfileAboutProps {
  /** Back to Profile & goals. */
  onBack: () => void;
}

/**
 * `/profile/about`: "Tell me about you", in the app frame. Opening it makes
 * the portrait (one model call per profile version); the spend line says
 * what the month has cost.
 */
export function ProfileAbout({ onBack }: ProfileAboutProps) {
  return <AboutYouPage onBack={onBack} spend={<SpendLine testId="about-spend" />} />;
}
