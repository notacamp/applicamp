import { Children, type ReactNode } from "react";

import styles from "./CardList.module.css";

export interface CardListProps {
  /** Names the list for screen readers ("All opportunities"). */
  label: string;
  /** The cards ({@link BoardCard}), one per row. */
  children: ReactNode;
  testId?: string | undefined;
}

/** A table's rows as cards, one under the other: the phone's form of a database view. */
export function CardList({ label, children, testId }: CardListProps) {
  return (
    <ul className={styles["list"]} aria-label={label} data-testid={testId}>
      {Children.map(children, (child) => (
        <li className={styles["item"]}>{child}</li>
      ))}
    </ul>
  );
}
