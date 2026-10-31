import type { ReactNode } from "react";

import styles from "./ChoiceNav.module.css";

export interface ChoiceNavItem {
  id: string;
  /** What it is ("Senior engineer"). */
  title: string;
  /** The muted line under it ("Example Org · 2021-now · in progress"). */
  meta?: ReactNode | undefined;
  /** This is the one open. */
  current?: boolean | undefined;
  /** For tests: `data-testid` of the choice. */
  testId?: string | undefined;
}

export interface ChoiceNavProps {
  /** Names the list for screen readers ("Your roles"). */
  label: string;
  items: readonly ChoiceNavItem[];
  onSelect: (id: string) => void;
  /** For tests: `data-testid` of the list. */
  testId?: string | undefined;
}

/**
 * A list of things to choose between, one open at a time: a column of
 * buttons on a wide screen, a row that scrolls sideways on a phone.
 */
export function ChoiceNav({ label, items, onSelect, testId }: ChoiceNavProps) {
  return (
    <nav className={styles["nav"]} aria-label={label} data-testid={testId}>
      <ul className={styles["list"]}>
        {items.map((item) => (
          <li key={item.id} className={styles["item"]}>
            <button
              type="button"
              className={styles["choice"]}
              aria-current={item.current === true ? "true" : undefined}
              data-testid={item.testId}
              onClick={() => {
                onSelect(item.id);
              }}
            >
              <span className={styles["title"]}>{item.title}</span>
              {item.meta === undefined ? null : <span className={styles["meta"]}>{item.meta}</span>}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}
