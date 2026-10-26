import { VisuallyHidden } from "@notacamp/ui";

import styles from "./MatchBar.module.css";

export interface MatchBarProps {
  /** 0 to 100; none when it was never scored. */
  value: number | null | undefined;
  /** What the number is, for screen readers ("Match"). */
  label: string;
  /** What to show when there is no score ("–"). */
  empty?: string | undefined;
}

/** How well something matches: the number and a short bar it fills. */
export function MatchBar({ value, label, empty = "–" }: MatchBarProps) {
  if (value === null || value === undefined) {
    return <span className={styles["empty"]}>{empty}</span>;
  }
  const clamped = Math.min(100, Math.max(0, Math.round(value)));
  return (
    <span className={styles["match"]}>
      <span className={styles["bar"]} aria-hidden="true">
        <span className={styles["fill"]} style={{ width: `${String(clamped)}%` }} />
      </span>
      <span className={styles["number"]}>
        <VisuallyHidden>{`${label} `}</VisuallyHidden>
        {clamped}
      </span>
    </span>
  );
}
