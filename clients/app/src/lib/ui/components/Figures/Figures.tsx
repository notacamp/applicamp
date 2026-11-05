import styles from "./Figures.module.css";

/** One figure of {@link Figures}: its number, already formatted, and what it counts. */
export interface Figure {
  /** "1,112", "€0.09". */
  value: string;
  /** "jobs read". */
  label: string;
  /** For tests: `data-testid` of the figure. */
  testId?: string | undefined;
}

export interface FiguresProps {
  items: readonly Figure[];
  /** Smaller numbers, for a row among others (an earlier search). */
  compact?: boolean | undefined;
  testId?: string | undefined;
}

/**
 * A few numbers side by side, each over what it counts ("1,112 / jobs
 * read"): what a search did, at a glance. They wrap onto more lines on a
 * phone.
 */
export function Figures({ items, compact = false, testId }: FiguresProps) {
  return (
    <dl className={styles["list"]} data-compact={compact ? "" : undefined} data-testid={testId}>
      {items.map((item) => (
        <div key={item.label} className={styles["figure"]} data-testid={item.testId}>
          <dt className={styles["label"]}>{item.label}</dt>
          <dd className={styles["value"]}>{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}
