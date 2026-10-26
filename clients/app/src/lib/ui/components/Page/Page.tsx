import { Heading } from "@notacamp/ui";
import type { ReactNode } from "react";

import styles from "./Page.module.css";

export interface PageProps {
  /** The page's name, its one h1 ("Pipeline"). */
  title: string;
  /** Small caps above the title (the date on Today). */
  eyebrow?: string | undefined;
  /** One or two sentences under the title: what this place is for. */
  lead?: string | undefined;
  /** For tests: `data-testid` of the page. */
  testId?: string | undefined;
  children?: ReactNode;
}

/** A place's page in the app frame: a calm, centred column, like a Notion page. */
export function Page({ title, eyebrow, lead, testId, children }: PageProps) {
  return (
    <main className={styles["page"]} data-testid={testId}>
      <div className={styles["column"]}>
        <header className={styles["head"]}>
          {eyebrow === undefined ? null : <p className={styles["eyebrow"]}>{eyebrow}</p>}
          <Heading level={1} size="hero">
            {title}
          </Heading>
          {lead === undefined ? null : <p className={styles["lead"]}>{lead}</p>}
        </header>
        {children}
      </div>
    </main>
  );
}
