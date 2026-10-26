import { VisuallyHidden } from "@notacamp/ui";
import { useEffect, useRef, type ReactNode } from "react";

import styles from "./DataTable.module.css";

/** One column of a {@link DataTable}. */
export interface DataColumn<T> {
  id: string;
  /** The heading ("Role"). */
  header: ReactNode;
  /** The cell of a row. */
  cell: (row: T) => ReactNode;
  /** Keep the cell on one line (dates, tags). */
  nowrap?: boolean | undefined;
}

export interface DataTableProps<T> {
  /** Names the table for screen readers ("All opportunities"). */
  label: string;
  /** The first column opens the row: it is drawn as the row's button. */
  columns: readonly DataColumn<T>[];
  rows: readonly T[];
  rowKey: (row: T) => string;
  /** The keyboard cursor (J/K): that row is highlighted and takes the focus. */
  selected?: string | null | undefined;
  onOpen: (row: T) => void;
  /** For tests: `data-testid` of each row and of its open button. */
  rowTestId?: string | undefined;
  openTestId?: string | undefined;
  testId?: string | undefined;
}

/**
 * A Notion-style database table: hairline rules, a quiet header, every row
 * opening its item. The first cell is a real button stretched over the row,
 * so a click anywhere on it, Enter or Space opens it. Scrolls sideways
 * inside itself on a narrow screen.
 */
export function DataTable<T>({
  label,
  columns,
  rows,
  rowKey,
  selected,
  onOpen,
  rowTestId,
  openTestId,
  testId,
}: DataTableProps<T>) {
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  const previous = useRef(selected);
  useEffect(() => {
    // Follow the cursor when it moves, never on the first render.
    if (selected !== previous.current && selected !== null && selected !== undefined) {
      const button = buttons.current.get(selected);
      button?.focus();
      button?.scrollIntoView({ block: "nearest" });
    }
    previous.current = selected;
  }, [selected]);

  const [first, ...rest] = columns;
  return (
    <div className={styles["scroll"]} data-testid={testId}>
      <table className={styles["table"]}>
        <caption>
          <VisuallyHidden>{label}</VisuallyHidden>
        </caption>
        <thead>
          <tr>
            {columns.map((column) => (
              <th key={column.id} scope="col" className={styles["head"]}>
                {column.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const key = rowKey(row);
            return (
              <tr
                key={key}
                className={styles["row"]}
                data-selected={key === selected ? "" : undefined}
                data-testid={rowTestId}
              >
                {first === undefined ? null : (
                  <td className={styles["cell"]} data-primary="">
                    <button
                      type="button"
                      className={styles["open"]}
                      data-testid={openTestId}
                      ref={(element) => {
                        if (element === null) {
                          buttons.current.delete(key);
                        } else {
                          buttons.current.set(key, element);
                        }
                      }}
                      onClick={() => {
                        onOpen(row);
                      }}
                    >
                      {first.cell(row)}
                    </button>
                  </td>
                )}
                {rest.map((column) => (
                  <td
                    key={column.id}
                    className={styles["cell"]}
                    data-nowrap={column.nowrap === true ? "" : undefined}
                  >
                    {column.cell(row)}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
