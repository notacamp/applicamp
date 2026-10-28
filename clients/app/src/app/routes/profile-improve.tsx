import { SessionGate } from "~/modules/identity";
import { ImproveProfile } from "~/modules/profile";

export interface ProfileImproveProps {
  /** The question asked for (`?q=`). */
  question: string | undefined;
  onLeave: (reason: "later" | "done") => void;
}

/**
 * `/profile/improve`: the AI's questions about the profile's gaps, one per
 * screen, outside the app frame (like the goals interview) but behind the
 * session. Never required: Finish later and Esc leave at any point.
 */
export function ProfileImprove({ question, onLeave }: ProfileImproveProps) {
  return (
    <SessionGate>
      <ImproveProfile question={question} onLeave={onLeave} />
    </SessionGate>
  );
}
