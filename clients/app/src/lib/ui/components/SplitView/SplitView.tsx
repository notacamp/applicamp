import type { ReactNode } from "react";

import styles from "./SplitView.module.css";

export interface SplitViewProps {
  /** The list column's accessible name ("Runs"). */
  listLabel: string;
  /** The detail column's accessible name ("Run"). */
  detailLabel: string;
  /** The list (left from 48rem). */
  list: ReactNode;
  /** What the selected item shows (right from 48rem). */
  detail: ReactNode;
  /**
   * Below 48rem only one column fits: the list until something is chosen,
   * then the detail (with `back` above it).
   */
  showing: "list" | "detail";
  /** Above the detail on phones only: the way back to the list. */
  back?: ReactNode | undefined;
}

/**
 * A list and the detail of its chosen item side by side, like a mail
 * client: one column at a time on a phone, both from 48rem.
 */
export function SplitView({ listLabel, detailLabel, list, detail, showing, back }: SplitViewProps) {
  return (
    <div className={styles["split"]} data-showing={showing}>
      <section className={styles["list"]} aria-label={listLabel}>
        {list}
      </section>
      <section className={styles["detail"]} aria-label={detailLabel}>
        {back === undefined ? null : <div className={styles["back"]}>{back}</div>}
        {detail}
      </section>
    </div>
  );
}
