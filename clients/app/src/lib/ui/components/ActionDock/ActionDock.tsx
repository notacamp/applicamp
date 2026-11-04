import type { ReactNode } from "react";

import styles from "./ActionDock.module.css";

export interface ActionDockProps {
  /** The page's main action, and the one beside it. */
  actions: ReactNode;
  /** One line under them: when the action turns on, or what it starts. */
  note?: ReactNode;
  testId?: string | undefined;
}

/**
 * The page's main action (Approve) in a bar stuck to the bottom of the
 * screen, above the phone's tab bar, with the line that says when it turns
 * on: always in reach however long the page is.
 */
export function ActionDock({ actions, note, testId }: ActionDockProps) {
  return (
    <div className={styles["dock"]} data-testid={testId}>
      <div className={styles["actions"]}>{actions}</div>
      {note}
    </div>
  );
}

/**
 * Buttons big enough to tap inside it: 44 px tall on a phone, 36 px on a
 * desktop.
 */
export function TouchTargets({ children }: { children: ReactNode }) {
  return <div className={styles["touch"]}>{children}</div>;
}
