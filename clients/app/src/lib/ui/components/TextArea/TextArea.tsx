import { useId, type ComponentPropsWithRef } from "react";

import styles from "./TextArea.module.css";

export type TextAreaProps = Omit<ComponentPropsWithRef<"textarea">, "className" | "style"> & {
  /** The visible label (always the accessible name). */
  label: string;
  /** Help under the control. */
  hint?: string | undefined;
  /** What is wrong with the value; marks the control invalid. */
  error?: string | undefined;
};

/**
 * A labelled multi-line field, styled like the package's TextField (which
 * has no multi-line form; NoteField takes the focus and the keyboard, for a
 * note on its own).
 */
export function TextArea({ label, hint, error, id, rows = 4, ...rest }: TextAreaProps) {
  const generated = useId();
  const controlId = id ?? generated;
  const hintId = `${controlId}-hint`;
  const errorId = `${controlId}-error`;
  const described = [
    ...(hint === undefined ? [] : [hintId]),
    ...(error === undefined ? [] : [errorId]),
  ];
  const describedBy = described.length > 0 ? described.join(" ") : undefined;
  return (
    <div className={styles["field"]}>
      <label className={styles["label"]} htmlFor={controlId}>
        {label}
      </label>
      <textarea
        id={controlId}
        className={styles["control"]}
        rows={rows}
        aria-invalid={error === undefined ? undefined : true}
        aria-describedby={describedBy}
        {...rest}
      />
      {hint === undefined ? null : (
        <p className={styles["hint"]} id={hintId}>
          {hint}
        </p>
      )}
      {error === undefined ? null : (
        <p className={styles["error"]} id={errorId}>
          {error}
        </p>
      )}
    </div>
  );
}
