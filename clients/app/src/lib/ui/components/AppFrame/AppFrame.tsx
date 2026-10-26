import { Shell } from "@notacamp/ui";
import type { ReactNode } from "react";

import styles from "./AppFrame.module.css";

/**
 * The app frame: @notacamp/ui's Shell (sidebar and page side by side; the
 * phone header and tab bar below 48rem), its sidebar on the sunken surface.
 */
export function AppFrame({ children }: { children: ReactNode }) {
  return (
    <div className={styles.frame}>
      <Shell>{children}</Shell>
    </div>
  );
}
