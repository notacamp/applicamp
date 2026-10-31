import type { ReactNode } from "react";

import styles from "./Portrait.module.css";

/** Where a sentence of the portrait comes from, and what became of it. */
export type PortraitKind =
  | "known"
  | "told"
  | "assumed"
  /** An assumption the person said is right. */
  | "confirmed"
  /** An assumption the person said is wrong: struck out. */
  | "wrong";

export interface PortraitProps {
  /** What it is (its accessible name). */
  label: string;
  /** The sentences, as {@link PortraitSentence}s. */
  children: ReactNode;
  /** For tests: `data-testid` of the box. */
  testId?: string | undefined;
}

/** A portrait: one written text whose sentences are marked by where each comes from. */
export function Portrait({ label, children, testId }: PortraitProps) {
  return (
    <article className={styles["portrait"]} aria-label={label} data-testid={testId}>
      <p className={styles["text"]}>{children}</p>
    </article>
  );
}

export interface PortraitSentenceProps {
  kind: PortraitKind;
  /** Told just now, from what the person wrote: outlined. */
  fresh?: boolean | undefined;
  /** The sentence. */
  children: ReactNode;
  /** Makes it a button (an assumption to settle); its panel is open when `expanded`. */
  onSelect?: (() => void) | undefined;
  expanded?: boolean | undefined;
  /** For tests: `data-testid` of the sentence. */
  testId?: string | undefined;
}

/** One sentence of the portrait; an assumption is a button that opens its panel. */
export function PortraitSentence({
  kind,
  fresh = false,
  children,
  onSelect,
  expanded,
  testId,
}: PortraitSentenceProps) {
  const flags = { "data-kind": kind, "data-fresh": fresh ? "" : undefined, "data-testid": testId };
  return (
    <>
      {onSelect === undefined ? (
        <span className={styles["sentence"]} {...flags}>
          {children}
        </span>
      ) : (
        <button
          type="button"
          className={styles["sentence"]}
          aria-expanded={expanded === true}
          onClick={onSelect}
          {...flags}
        >
          {children}
        </button>
      )}{" "}
    </>
  );
}

export interface PortraitLegendProps {
  /** What the swatches mean. */
  label: string;
  items: readonly { kind: PortraitKind; label: string }[];
}

/** The key to a portrait's marks. */
export function PortraitLegend({ label, items }: PortraitLegendProps) {
  return (
    <ul className={styles["legend"]} aria-label={label}>
      {items.map((item) => (
        <li key={item.kind} className={styles["legend-item"]}>
          <span className={styles["swatch"]} data-kind={item.kind} aria-hidden="true" />
          {item.label}
        </li>
      ))}
    </ul>
  );
}

/** The small panel under a portrait where one assumption is settled. */
export function PortraitPanel({
  label,
  children,
  testId,
}: {
  label: string;
  children: ReactNode;
  testId?: string | undefined;
}) {
  return (
    <section className={styles["panel"]} aria-label={label} data-testid={testId}>
      {children}
    </section>
  );
}
