import { CompletePage } from "~/modules/profile";

export interface ProfileCompleteProps {
  /** Back to Profile & goals. */
  onBack: () => void;
}

/**
 * `/profile/complete`: "Complete your profile", in the app frame. The facts
 * the app already knows that the profile does not hold yet, to keep or drop.
 * Opening it calls no model.
 */
export function ProfileComplete({ onBack }: ProfileCompleteProps) {
  return <CompletePage onBack={onBack} />;
}
