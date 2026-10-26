import type { ReactNode } from "react";

import type { DataAttributes } from "../Tagged/Tagged";

import styles from "./PickList.module.css";

/** A list of rows to choose one from (the runs beside their console). */
export function PickList({
  label,
  testId,
  children,
}: {
  /** The list's accessible name ("Runs"). */
  label: string;
  testId?: string | undefined;
  children: ReactNode;
}) {
  return (
    <ul className={styles["list"]} aria-label={label} data-testid={testId}>
      {children}
    </ul>
  );
}

export interface PickRowProps {
  /** Before the title: a status dot, an icon. */
  leading?: ReactNode | undefined;
  /** The row's name ("Check"). */
  title: ReactNode;
  /** The quiet second line (a status, a time). */
  meta?: ReactNode | undefined;
  /** The chosen row: highlighted and `aria-current`. */
  selected?: boolean | undefined;
  onSelect: () => void;
  testId?: string | undefined;
  /** `data-*` attributes a test reads. */
  data?: DataAttributes | undefined;
}

/** One row of a {@link PickList}: a real button that chooses it. */
export function PickRow({ leading, title, meta, selected, onSelect, testId, data }: PickRowProps) {
  return (
    <li className={styles["item"]}>
      <button
        type="button"
        className={styles["row"]}
        aria-current={selected === true ? "true" : undefined}
        data-testid={testId}
        {...data}
        onClick={onSelect}
      >
        {leading === undefined ? null : <span className={styles["leading"]}>{leading}</span>}
        <span className={styles["text"]}>
          <span className={styles["title"]}>{title}</span>
          {meta === undefined ? null : <span className={styles["meta"]}>{meta}</span>}
        </span>
      </button>
    </li>
  );
}
