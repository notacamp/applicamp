import type { ReactNode } from "react";

import styles from "./Chat.module.css";

/** A conversation, newest at the bottom, announced politely as it grows. */
export function Chat({
  label,
  children,
  testId,
}: {
  /** Names the conversation for screen readers. */
  label: string;
  children: ReactNode;
  testId?: string | undefined;
}) {
  return (
    <div className={styles["chat"]} role="log" aria-label={label} data-testid={testId}>
      {children}
    </div>
  );
}

export interface ChatBubbleProps {
  /**
   * `question` is asked of the person, `push` is a question that pushes back
   * (marked with a bar), `answer` is theirs, at the right.
   */
  kind: "question" | "push" | "answer";
  /** Who is speaking, small above the words ("Pushing back"). */
  label?: string | undefined;
  children: ReactNode;
  /** For tests: `data-testid` of the bubble. */
  testId?: string | undefined;
}

/** One turn of a conversation. */
export function ChatBubble({ kind, label, children, testId }: ChatBubbleProps) {
  return (
    <div className={styles["bubble"]} data-kind={kind} data-testid={testId}>
      {label === undefined ? null : <p className={styles["who"]}>{label}</p>}
      <p className={styles["words"]}>{children}</p>
    </div>
  );
}
