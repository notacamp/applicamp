import type { ReactNode } from "react";

import styles from "./Label.module.css";

/** Notion's tag colours, each a soft fill under the primary text. */
export const LABEL_HUES = [
  "gray",
  "brown",
  "orange",
  "yellow",
  "green",
  "blue",
  "purple",
  "pink",
  "red",
] as const;
export type LabelHue = (typeof LABEL_HUES)[number];

export interface LabelProps {
  /** The fill; the words carry the meaning, the colour only repeats it. */
  hue: LabelHue;
  children: ReactNode;
}

/** A small coloured label, like a Notion select value (a stage, a source). */
export function Label({ hue, children }: LabelProps) {
  return (
    <span className={styles["label"]} data-hue={hue}>
      {children}
    </span>
  );
}
