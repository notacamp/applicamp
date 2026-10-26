import { Button, NoteField, TextField } from "@notacamp/ui";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import styles from "./InlineText.module.css";

export interface InlineTextProps {
  /** What the value is ("Headline"): the field's label, for screen readers. */
  label: string;
  /** The value now; none shows the placeholder. */
  value: string | undefined;
  /** Shown, muted, when there is no value ("Add a headline"). */
  placeholder: string;
  /** Keep the new value (trimmed; empty clears it). */
  onSave: (value: string) => void;
  /** A paragraph rather than a line (a summary): ⌘↵ keeps it. */
  multiline?: boolean | undefined;
  /** For a multiline value: its longest length, and the counter near it. */
  maxLength?: number | undefined;
  counter?: ((left: number) => string) | undefined;
  saveLabel: string;
  cancelLabel: string;
  /** While the save runs: the buttons wait. */
  busy?: boolean | undefined;
  /** For tests: the value's button; `-input`, `-save` and `-cancel` for the editor. */
  testId: string;
}

/**
 * A value edited in place, like a Notion property: it reads as text, a click
 * (or Enter) opens a field, Enter keeps it and Esc leaves it as it was.
 */
export function InlineText({
  label,
  value,
  placeholder,
  onSave,
  multiline = false,
  maxLength = 10_000,
  counter = String,
  saveLabel,
  cancelLabel,
  busy = false,
  testId,
}: InlineTextProps) {
  const [draft, setDraft] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const editing = draft !== null;

  useEffect(() => {
    if (editing && !multiline) {
      input.current?.focus();
    }
  }, [editing, multiline]);

  if (draft === null) {
    return (
      <button
        type="button"
        className={styles["value"]}
        data-empty={value === undefined || value === "" ? "true" : undefined}
        data-multiline={multiline ? "true" : undefined}
        aria-label={`${label}: ${value ?? placeholder}`}
        data-testid={testId}
        onClick={() => {
          setDraft(value ?? "");
        }}
      >
        {value === undefined || value === "" ? placeholder : value}
      </button>
    );
  }

  const save = () => {
    onSave(draft.trim());
    setDraft(null);
  };
  const cancel = () => {
    setDraft(null);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      save();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancel();
    }
  };

  return (
    <div className={styles["editor"]}>
      {multiline ? (
        <NoteField
          label={label}
          value={draft}
          onChange={setDraft}
          maxLength={maxLength}
          counter={counter}
          onSubmit={save}
          onCancel={cancel}
          disabled={busy}
        />
      ) : (
        <TextField
          ref={input}
          label={label}
          hideLabel
          value={draft}
          disabled={busy}
          data-testid={`${testId}-input`}
          onChange={(event) => {
            setDraft(event.target.value);
          }}
          onKeyDown={onKeyDown}
        />
      )}
      <div className={styles["actions"]}>
        <Button variant="primary" data-testid={`${testId}-save`} disabled={busy} onClick={save}>
          {saveLabel}
        </Button>
        <Button variant="ghost" data-testid={`${testId}-cancel`} disabled={busy} onClick={cancel}>
          {cancelLabel}
        </Button>
      </div>
    </div>
  );
}
