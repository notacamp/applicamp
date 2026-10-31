import type { ReactNode } from "react";

import styles from "./Workbench.module.css";

export interface WorkbenchProps {
  /** The list of things to work on (roles), at the left; a scroller above everything on narrow screens. */
  nav: ReactNode;
  /** A card beside the work (coverage), at the right; above the work on narrow screens. */
  aside?: ReactNode;
  /** The work itself (a conversation), in the middle. */
  children: ReactNode;
  /** For tests: `data-testid` of the workbench. */
  testId?: string | undefined;
}

/**
 * Three columns: a list, the work, a card. When the page is too narrow for
 * all three (a phone, or a laptop beside the sidebar: it asks its own width,
 * not the screen's) they stack in one column: the list, then the card, then
 * the work, so the card is seen without scrolling past the whole conversation.
 */
export function Workbench({ nav, aside, children, testId }: WorkbenchProps) {
  return (
    <div className={styles["frame"]}>
      <div
        className={styles["workbench"]}
        data-aside={aside === undefined ? "none" : "some"}
        data-testid={testId}
      >
        <div className={styles["nav"]}>{nav}</div>
        <div className={styles["main"]}>{children}</div>
        {aside === undefined ? null : <aside className={styles["aside"]}>{aside}</aside>}
      </div>
    </div>
  );
}
