import { SessionGate } from "~/modules/identity";
import { GoalInterview, type Question } from "~/modules/profile";

export interface GoalsInterviewProps {
  /** The question on screen (`?step=`). */
  question: Question | undefined;
  onQuestion: (question: Question) => void;
  onLeave: (reason: "later" | "done") => void;
}

/**
 * `/goals/interview`: the goals questions, one per screen, outside the app
 * frame (like sign-in) but behind the session. Never required: leaving goes
 * to Today (later) or to Profile & goals (all asked).
 */
export function GoalsInterview({ question, onQuestion, onLeave }: GoalsInterviewProps) {
  return (
    <SessionGate>
      <GoalInterview question={question} onQuestion={onQuestion} onLeave={onLeave} />
    </SessionGate>
  );
}
