import type { ReactNode } from "react";

import styles from "./Tagged.module.css";

/**
 * Gives a package component that takes no `data-testid` (FocusPage, Banner,
 * NavItem) one, without changing the layout: tests locate elements by
 * `data-testid` only.
 */
export function Tagged({ testId, children }: { testId: string; children: ReactNode }) {
  return (
    <div className={styles.tagged} data-testid={testId}>
      {children}
    </div>
  );
}
