import { Heading } from "@notacamp/ui";
import { useId, type ReactNode } from "react";

import styles from "./Board.module.css";

export interface BoardProps {
  /** Names the board for screen readers ("By stage"). */
  label: string;
  /** The columns ({@link BoardColumn}), left to right. */
  children: ReactNode;
  testId?: string | undefined;
}

/**
 * A board of columns, like Notion's board view. Its row of columns scrolls
 * sideways inside itself, so a phone never scrolls the page sideways.
 */
export function Board({ label, children, testId }: BoardProps) {
  return (
    <section className={styles["board"]} aria-label={label} data-testid={testId}>
      {children}
    </section>
  );
}

export interface BoardColumnProps {
  /** Its heading: a label and how many cards it holds. */
  title: ReactNode;
  count: number;
  /** Shown when it holds nothing ("Nothing here"). */
  empty: string;
  children?: ReactNode;
  testId?: string | undefined;
}

/** One column of a {@link Board}: its heading, then its cards. */
export function BoardColumn({ title, count, empty, children, testId }: BoardColumnProps) {
  const id = useId();
  return (
    <section className={styles["column"]} aria-labelledby={id} data-testid={testId}>
      <Heading level={3} size="section" id={id}>
        <span className={styles["columnHead"]}>
          {title}
          <span className={styles["count"]}>{count}</span>
        </span>
      </Heading>
      {count === 0 ? <p className={styles["empty"]}>{empty}</p> : children}
    </section>
  );
}

export interface BoardCardProps {
  /** The card's name (the role). */
  title: string;
  /** One quieter line under it (the company and the place). */
  subtitle?: string | undefined;
  /** Tags and figures, in one wrapping row. */
  meta?: ReactNode;
  /** The keyboard cursor is here. */
  selected?: boolean | undefined;
  onOpen: () => void;
  testId?: string | undefined;
}

/** A card that opens its item: on a board, and as a row of a phone's list. */
export function BoardCard({
  title,
  subtitle,
  meta,
  selected = false,
  onOpen,
  testId,
}: BoardCardProps) {
  return (
    <button
      type="button"
      className={styles["card"]}
      data-selected={selected ? "" : undefined}
      data-testid={testId}
      onClick={onOpen}
    >
      <span className={styles["title"]}>{title}</span>
      {subtitle === undefined ? null : <span className={styles["subtitle"]}>{subtitle}</span>}
      {meta === undefined ? null : <span className={styles["meta"]}>{meta}</span>}
    </button>
  );
}
