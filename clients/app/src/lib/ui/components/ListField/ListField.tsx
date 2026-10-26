import { Button, IconButton, KbdHint, Tag, TextField } from "@notacamp/ui";
import type { KeyboardEvent } from "react";

import styles from "./ListField.module.css";

/**
 * `items` with `draft` added: trimmed, and unchanged when the draft is
 * blank, already there (any case) or the list is full.
 */
export function withItem(items: readonly string[], draft: string, max: number): string[] {
  const item = draft.trim().replaceAll(/\s+/g, " ");
  const known = items.some((existing) => existing.toLowerCase() === item.toLowerCase());
  return item === "" || known || items.length >= max ? [...items] : [...items, item];
}

export interface ListFieldProps {
  /** What the list is ("Roles"): the field's label. */
  label: string;
  /** Help under the field ("Up to 5."), before the Shift+Enter hint. */
  hint?: string | undefined;
  /** What Shift+Enter does, after its keys ("adds another"). */
  addHint: string;
  /** The items so far. */
  items: readonly string[];
  onItems: (items: string[]) => void;
  /** What is typed and not yet added. The owner keeps it, to add it on Next. */
  draft: string;
  onDraft: (draft: string) => void;
  /** Most items; the field waits when it is full. */
  max: number;
  /** Longest item. */
  maxLength: number;
  placeholder?: string | undefined;
  addLabel: string;
  /** The remove button's name for `item` ("Remove Staff engineer"). */
  removeLabel: (item: string) => string;
  /** For tests: the field is `-input`; `-add`, `-item-N` and `-remove-N` beside it. */
  testId: string;
}

/**
 * A list of short texts built one at a time: type, then Add or Shift+Enter
 * (a plain Enter belongs to the form around it). Each item is a tag with a
 * button to remove it.
 */
export function ListField({
  label,
  hint,
  addHint,
  items,
  onItems,
  draft,
  onDraft,
  max,
  maxLength,
  placeholder,
  addLabel,
  removeLabel,
  testId,
}: ListFieldProps) {
  const full = items.length >= max;
  const add = () => {
    onItems(withItem(items, draft, max));
    onDraft("");
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" && event.shiftKey) {
      event.preventDefault();
      add();
    }
  };
  return (
    <div className={styles["field"]}>
      <div className={styles["entry"]}>
        <div className={styles["input"]}>
          <TextField
            label={label}
            size="lg"
            value={draft}
            maxLength={maxLength}
            placeholder={placeholder}
            disabled={full}
            hint={
              <span className={styles["hint"]}>
                {hint === undefined ? null : <span>{hint}</span>}
                <KbdHint keys="shift+enter" />
                <span>{addHint}</span>
              </span>
            }
            data-testid={`${testId}-input`}
            onChange={(event) => {
              onDraft(event.target.value);
            }}
            onKeyDown={onKeyDown}
          />
        </div>
        <Button
          variant="secondary"
          size="lg"
          icon="plus"
          disabled={full || draft.trim() === ""}
          data-testid={`${testId}-add`}
          onClick={add}
        >
          {addLabel}
        </Button>
      </div>
      {items.length === 0 ? null : (
        <ul className={styles["items"]}>
          {items.map((item, index) => (
            <li
              key={item}
              className={styles["item"]}
              data-testid={`${testId}-item-${String(index)}`}
            >
              <Tag>{item}</Tag>
              <IconButton
                icon="close"
                label={removeLabel(item)}
                data-testid={`${testId}-remove-${String(index)}`}
                onClick={() => {
                  onItems(items.filter((_, at) => at !== index));
                }}
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
