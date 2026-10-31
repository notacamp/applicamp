import styles from "./StatusMark.module.css";

export interface StatusMarkProps {
  /** `ok` is done (a tick), `part` is half done, `none` is not yet. */
  state: "ok" | "part" | "none";
  /** What it says for screen readers ("Covered", "Partly covered", "Not covered yet"). */
  label: string;
}

const GLYPHS = { ok: "✓", part: "½", none: "!" } as const;

/** A small round mark for how far something is, which does not rely on colour alone. */
export function StatusMark({ state, label }: StatusMarkProps) {
  return (
    <span className={styles["mark"]} data-state={state} role="img" aria-label={label}>
      <span aria-hidden="true">{GLYPHS[state]}</span>
    </span>
  );
}
