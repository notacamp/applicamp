import type { ReactNode } from "react";

import type { DataAttributes } from "../Tagged/Tagged";

import styles from "./EventLog.module.css";

export interface EventLogProps {
  /** The log's accessible name ("What it's doing"). */
  label: string;
  /** The lines ({@link EventLine}), oldest first. */
  children: ReactNode;
  testId?: string | undefined;
}

/**
 * Lines written as something happens (an agent's steps, a sandbox's
 * network log), oldest first. A `log` region: new lines are announced
 * politely as they arrive.
 */
export function EventLog({ label, children, testId }: EventLogProps) {
  return (
    <div role="log" aria-label={label} data-testid={testId}>
      <ol className={styles["log"]}>{children}</ol>
    </div>
  );
}

export interface EventLineProps {
  /** When it happened (ISO 8601), for `<time>`. */
  at: string;
  /** The same, as shown ("14:02:07"). */
  atText: string;
  /** Before the text: what kind of line it is, in words (a Label: "Blocked"). */
  badge?: ReactNode | undefined;
  /** The line itself, shown as written (never markup). */
  children: string;
  /** Machine-written text (a proxy's log line): monospace. */
  mono?: boolean | undefined;
  testId?: string | undefined;
  /** `data-*` attributes a test reads. */
  data?: DataAttributes | undefined;
}

/** One line of an {@link EventLog}: its time, what kind it is, and the text. */
export function EventLine({ at, atText, badge, children, mono, testId, data }: EventLineProps) {
  return (
    <li
      className={styles["line"]}
      data-mono={mono === true ? "" : undefined}
      data-testid={testId}
      {...data}
    >
      <time className={styles["time"]} dateTime={at}>
        {atText}
      </time>
      {badge}
      <span className={styles["message"]}>{children}</span>
    </li>
  );
}
