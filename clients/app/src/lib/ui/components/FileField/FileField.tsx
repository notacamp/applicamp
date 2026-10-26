import { Icon } from "@notacamp/ui";
import { useId, type ChangeEvent } from "react";

import styles from "./FileField.module.css";

export interface FileFieldProps {
  /** What to choose, as the field's visible name ("Your LinkedIn export (.zip)"). */
  label: string;
  /** Under it: where the file comes from, its limit. */
  hint?: string | undefined;
  /** The file types offered (`.zip,application/zip`). */
  accept: string;
  /** The file chosen, if any. */
  file: File | null;
  onChange: (file: File | null) => void;
  /** Shown before a file is chosen ("Choose a file"). */
  chooseLabel: string;
  disabled?: boolean | undefined;
  /** For tests: `data-testid` of the file input (Playwright sets files on it). */
  testId: string;
}

/**
 * A file picker as a quiet dashed panel: the native input (keyboard, screen
 * readers, a phone's file sheet), visually hidden, with the chosen file's
 * name in its place.
 */
export function FileField({
  label,
  hint,
  accept,
  file,
  onChange,
  chooseLabel,
  disabled = false,
  testId,
}: FileFieldProps) {
  const hintId = useId();
  return (
    <div className={styles["field"]}>
      <label className={styles["drop"]} data-disabled={disabled ? "true" : undefined}>
        <input
          type="file"
          className={styles["input"]}
          accept={accept}
          disabled={disabled}
          aria-describedby={hint === undefined ? undefined : hintId}
          data-testid={testId}
          onChange={(event: ChangeEvent<HTMLInputElement>) => {
            onChange(event.target.files?.item(0) ?? null);
          }}
        />
        <span className={styles["icon"]}>
          <Icon name="attachment" />
        </span>
        <span className={styles["text"]}>
          <span className={styles["label"]}>{label}</span>
          <span className={styles["file"]}>{file === null ? chooseLabel : file.name}</span>
        </span>
      </label>
      {hint === undefined ? null : (
        <p className={styles["hint"]} id={hintId}>
          {hint}
        </p>
      )}
    </div>
  );
}
