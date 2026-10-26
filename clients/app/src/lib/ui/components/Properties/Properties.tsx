import type { ReactNode } from "react";

import styles from "./Properties.module.css";

/**
 * Notion-style properties: a quiet label column and the values beside it
 * (a narrower label column on phones). Not a card: they sit on the page.
 */
export function PropertyList({ children, testId }: { children: ReactNode; testId?: string }) {
  return (
    <dl className={styles["list"]} data-testid={testId}>
      {children}
    </dl>
  );
}

export interface PropertyProps {
  /** What the value is ("Headline"). */
  label: ReactNode;
  /** The value: text, tags, or a value edited in place. */
  children: ReactNode;
  /** For tests: `data-testid` of the value. */
  testId?: string | undefined;
}

/** One row of a {@link PropertyList}. */
export function Property({ label, children, testId }: PropertyProps) {
  return (
    <div className={styles["row"]}>
      <dt className={styles["label"]}>{label}</dt>
      <dd className={styles["value"]} data-testid={testId}>
        {children}
      </dd>
    </div>
  );
}
