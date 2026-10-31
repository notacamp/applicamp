import type { ReactNode } from "react";

import styles from "./FactBlock.module.css";

export interface FactBlockProps {
  /** What the block holds ("Facts taken from your answer"). */
  title: string;
  /** The rows. */
  children: ReactNode;
  /** Under the rows: what happens to them, and Keep all. */
  footer?: ReactNode | undefined;
  /** For tests: `data-testid` of the block. */
  testId?: string | undefined;
}

/** A box of facts, each a {@link FactRow}, with a line under them. */
export function FactBlock({ title, children, footer, testId }: FactBlockProps) {
  return (
    <section className={styles["block"]} aria-label={title} data-testid={testId}>
      <h3 className={styles["title"]}>{title}</h3>
      <div className={styles["rows"]}>{children}</div>
      {footer === undefined ? null : <div className={styles["footer"]}>{footer}</div>}
    </section>
  );
}

export interface FactRowProps {
  /** The fact, one short sentence. */
  text: string;
  /** "Your words", before the quote. */
  quoteLabel: string;
  /** The person's own words it comes from. */
  quote: string;
  /** What can be done with it (Keep, Fix, Drop; or its state). */
  actions?: ReactNode | undefined;
  /** Under the row, full width (an editor, an error). */
  children?: ReactNode;
  /** Dimmed: a fact that is not going anywhere (dropped). */
  muted?: boolean | undefined;
  /** For tests: `data-testid` of the row. */
  testId?: string | undefined;
}

/** One fact, with the person's own words under it. */
export function FactRow({
  text,
  quoteLabel,
  quote,
  actions,
  children,
  muted = false,
  testId,
}: FactRowProps) {
  return (
    <div className={styles["row"]} data-muted={muted ? "" : undefined} data-testid={testId}>
      <div className={styles["body"]}>
        <p className={styles["text"]}>{text}</p>
        <p className={styles["quote"]}>
          {quoteLabel} <q>{quote}</q>
        </p>
      </div>
      {actions === undefined ? null : <div className={styles["actions"]}>{actions}</div>}
      {children === undefined ? null : <div className={styles["under"]}>{children}</div>}
    </div>
  );
}
