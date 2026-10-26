import { Button } from "@notacamp/ui";
import { useEffect, type ReactNode, type SubmitEvent } from "react";

import styles from "./InlineEdit.module.css";

export interface InlineEditProps {
  /** What the value is ("Roles"): the button's name for screen readers. */
  label: string;
  /** The value as text; none shows the placeholder. */
  summary: string | undefined;
  /** Shown, muted, when there is no value ("Add your roles"). */
  placeholder: string;
  /** Whether the editor is open. */
  editing: boolean;
  onEdit: () => void;
  /** Enter (or Save) keeps the edit. */
  onSave: () => void;
  /** Esc (or Cancel) leaves the value as it was. */
  onCancel: () => void;
  saveLabel: string;
  cancelLabel: string;
  /** While the save runs: the buttons wait. */
  busy?: boolean | undefined;
  /** The editor: whatever controls change the value. */
  children: ReactNode;
  /** For tests: the value's button; `-save` and `-cancel` for the editor. */
  testId: string;
}

/**
 * A value edited in place, like a Notion property, with any controls as its
 * editor (a list, choices, an amount): it reads as text, a click opens the
 * editor, Enter or Save keeps it and Esc leaves it as it was.
 */
export function InlineEdit({
  label,
  summary,
  placeholder,
  editing,
  onEdit,
  onSave,
  onCancel,
  saveLabel,
  cancelLabel,
  busy = false,
  children,
  testId,
}: InlineEditProps) {
  useEffect(() => {
    if (!editing) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onCancel();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [editing, onCancel]);

  if (!editing) {
    const empty = summary === undefined || summary === "";
    return (
      <button
        type="button"
        className={styles["value"]}
        data-empty={empty ? "true" : undefined}
        aria-label={`${label}: ${empty ? placeholder : summary}`}
        data-testid={testId}
        onClick={onEdit}
      >
        {empty ? placeholder : summary}
      </button>
    );
  }
  const submit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    onSave();
  };
  return (
    <form className={styles["editor"]} aria-label={label} onSubmit={submit}>
      {children}
      <div className={styles["actions"]}>
        <Button variant="primary" type="submit" disabled={busy} data-testid={`${testId}-save`}>
          {saveLabel}
        </Button>
        <Button variant="ghost" disabled={busy} data-testid={`${testId}-cancel`} onClick={onCancel}>
          {cancelLabel}
        </Button>
      </div>
    </form>
  );
}
