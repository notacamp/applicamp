import { Heading } from "@notacamp/ui";
import type { ReactNode } from "react";

import styles from "./Page.module.css";

export interface PageProps {
  /** The page's name, its one h1 ("Jobs"). */
  title: string;
  /** Small caps above the title (the date on Today). */
  eyebrow?: string | undefined;
  /** One line under the title, when a page has one to say (Today's). */
  lead?: string | undefined;
  /** For tests: `data-testid` of the page. */
  testId?: string | undefined;
  /** A database page (Jobs): a wider column for its table and board. */
  wide?: boolean | undefined;
  children?: ReactNode;
}

/** A page in the app frame: a calm, centred column. */
export function Page({ title, eyebrow, lead, testId, wide = false, children }: PageProps) {
  return (
    <main className={styles["page"]} data-testid={testId}>
      <div className={styles["column"]} data-wide={wide ? "" : undefined}>
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
