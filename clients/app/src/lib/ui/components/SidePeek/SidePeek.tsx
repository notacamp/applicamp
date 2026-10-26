import { IconButton } from "@notacamp/ui";
import { useShortcut } from "@notacamp/ui/shortcuts";
import { useEffect, useRef, type ReactNode } from "react";

import styles from "./SidePeek.module.css";

export interface SidePeekProps {
  open: boolean;
  /** Names the panel (the item's title). */
  label: string;
  /** The small line at its top ("Opportunity"). */
  eyebrow?: string | undefined;
  /** The close button's name ("Close"). */
  closeLabel: string;
  /** Close, Escape. */
  onClose: () => void;
  children?: ReactNode;
  testId?: string | undefined;
  closeTestId?: string | undefined;
}

/**
 * Notion's side peek: an item opened beside its list, which stays usable.
 * The whole screen below 48rem. Opening moves the focus to its close button;
 * Escape or Close shut it.
 */
export function SidePeek({
  open,
  label,
  eyebrow,
  closeLabel,
  onClose,
  children,
  testId,
  closeTestId,
}: SidePeekProps) {
  const close = useRef<HTMLButtonElement>(null);
  useShortcut("overlay.close", onClose, { enabled: open });
  useEffect(() => {
    if (open) {
      close.current?.focus();
    }
  }, [open]);
  if (!open) {
    return null;
  }
  return (
    <div
      className={styles["peek"]}
      role="dialog"
      aria-modal="false"
      aria-label={label}
      data-testid={testId}
    >
      <div className={styles["top"]}>
        <span className={styles["eyebrow"]}>{eyebrow}</span>
        <IconButton
          ref={close}
          icon="close"
          label={closeLabel}
          shortcut="overlay.close"
          data-testid={closeTestId}
          onClick={onClose}
        />
      </div>
      <div className={styles["body"]}>{children}</div>
    </div>
  );
}
