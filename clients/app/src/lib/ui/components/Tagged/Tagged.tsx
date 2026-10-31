import type { ReactNode } from "react";

import styles from "./Tagged.module.css";

/** `data-*` attributes a test reads (`{ "data-status": "running" }`). */
export type DataAttributes = Partial<Record<`data-${string}`, string>>;

/**
 * Gives a package component that takes no `data-testid` (FocusPage, Banner,
 * NavItem) one, without changing the layout: tests locate elements by
 * `data-testid` only. `data` adds the `data-*` attributes they read.
 */
export function Tagged({
  testId,
  data,
  children,
}: {
  testId: string;
  data?: DataAttributes | undefined;
  children: ReactNode;
}) {
  return (
    <div className={styles["tagged"]} data-testid={testId} {...data}>
      {children}
    </div>
  );
}
