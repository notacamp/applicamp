import type { ReactNode } from "react";

import styles from "./DecisionCard.module.css";

/** Where a proposal is: waiting, kept (green) or dropped (faded, struck out). */
export type DecisionState = "open" | "kept" | "dropped";

export interface DecisionCardProps {
  /** What is proposed: one fact, one skill. */
  text: ReactNode;
  /** Under it: where it comes from and the words it comes from. */
  meta?: ReactNode;
  /** Keep and Drop while it waits; its state and Undo once decided. */
  actions: ReactNode;
  state?: DecisionState | undefined;
  /** For tests: `data-testid` of the card. */
  testId?: string | undefined;
}

/**
 * One proposal to keep or drop, as a card: kept ones turn green, dropped ones
 * fade, and both stay where they are so the person can undo them. The buttons
 * sit beside the text on a wide screen and under it on a phone.
 */
export function DecisionCard({ text, meta, actions, state = "open", testId }: DecisionCardProps) {
  return (
    <div className={styles["card"]} data-state={state} data-testid={testId}>
      <div className={styles["body"]}>
        <p className={styles["text"]}>{text}</p>
        {meta === undefined ? null : <div className={styles["meta"]}>{meta}</div>}
      </div>
      <div className={styles["actions"]}>{actions}</div>
    </div>
  );
}

/** Proposals one under the other. */
export function DecisionList({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className={styles["list"]} role="group" aria-label={label}>
      {children}
    </div>
  );
}
