import type { ReactNode } from "react";

import styles from "./RowList.module.css";

export interface RowListProps {
  /** Names the list for screen readers ("Your CVs"). */
  label: string;
  /** The rows ({@link RowItem}). */
  children: ReactNode;
  testId?: string | undefined;
}

/** Records one under the other, each with its facts and its own actions. */
export function RowList({ label, children, testId }: RowListProps) {
  return (
    <ul className={styles["list"]} aria-label={label} data-testid={testId}>
      {children}
    </ul>
  );
}

export interface RowItemProps {
  /** What the record is ("Staff Engineer · Example Co"). */
  title: ReactNode;
  /** A quieter line under it. */
  subtitle?: ReactNode;
  /** Tags and figures in one wrapping row. */
  meta?: ReactNode;
  /** Its buttons: beside it on a wide screen, under it on a phone. */
  actions?: ReactNode;
  /** The record to look at first (the newest version). */
  current?: boolean | undefined;
  testId?: string | undefined;
}

/**
 * One record of a {@link RowList}. Unlike a table row it is not one big
 * button: it holds several (View, Download, Review), each its own target.
 */
export function RowItem({ title, subtitle, meta, actions, current = false, testId }: RowItemProps) {
  return (
    <li className={styles["item"]} data-current={current ? "" : undefined} data-testid={testId}>
      <div className={styles["main"]}>
        <div className={styles["title"]}>{title}</div>
        {subtitle === undefined ? null : <div className={styles["subtitle"]}>{subtitle}</div>}
        {meta === undefined ? null : <div className={styles["meta"]}>{meta}</div>}
      </div>
      {actions === undefined ? null : <div className={styles["actions"]}>{actions}</div>}
    </li>
  );
}
