import { Heading } from "@notacamp/ui";
import type { ReactNode } from "react";

import styles from "./CompanyCard.module.css";

export interface CompanyCardProps {
  /** The company's name, an h3. */
  title: string;
  /** A quiet line beside it: its site. */
  aside?: ReactNode;
  /** The reasons ({@link ReasonList}), a status line, and the actions. */
  children: ReactNode;
  /** For tests: `data-testid` of the card. */
  testId?: string | undefined;
}

/**
 * A company worth watching, as a card: its name and the few sentences that
 * say why, with room under them for what is happening and what to do. Its
 * actions wrap under the text of a narrow card, whatever the screen.
 */
export function CompanyCard({ title, aside, children, testId }: CompanyCardProps) {
  return (
    <article className={styles["card"]} data-testid={testId}>
      <div className={styles["head"]}>
        <Heading level={3} size="section">
          {title}
        </Heading>
        {aside === undefined || aside === null ? null : (
          <span className={styles["aside"]}>{aside}</span>
        )}
      </div>
      {children}
    </article>
  );
}

/** The reasons of a {@link CompanyCard}, each ticked. */
export function ReasonList({
  label,
  children,
  testId,
}: {
  /** Names the list for screen readers ("Why Northwind Stays"). */
  label: string;
  children: ReactNode;
  testId?: string | undefined;
}) {
  return (
    <ul className={styles["reasons"]} aria-label={label} data-testid={testId}>
      {children}
    </ul>
  );
}

export interface ReasonRowProps {
  /** The reason, one sentence. */
  text: ReactNode;
  /** Where it comes from, under the sentence; none while hidden. */
  sources?: readonly string[] | undefined;
  testId?: string | undefined;
}

/** One reason: ticked, with the postings (or facts) it comes from under it. */
export function ReasonRow({ text, sources, testId }: ReasonRowProps) {
  return (
    <li className={styles["reason"]} data-testid={testId}>
      <span className={styles["tick"]} aria-hidden="true" />
      <span className={styles["text"]}>
        {text}
        {sources === undefined || sources.length === 0 ? null : (
          <span className={styles["sources"]}>
            {sources.map((source, index) => (
              // The same posting can be cited twice; its place keeps it apart.
              <span key={`${String(index)}-${source}`} className={styles["source"]}>
                {source}
              </span>
            ))}
          </span>
        )}
      </span>
    </li>
  );
}

/** What is happening to a company ("Looking for their job board…"), in a quiet box. */
export function CompanyStatus({
  children,
  testId,
}: {
  children: ReactNode;
  testId?: string | undefined;
}) {
  return (
    <p className={styles["status"]} role="status" data-testid={testId}>
      {children}
    </p>
  );
}

/** A card's buttons, in one wrapping row. */
export function CompanyActions({ children }: { children: ReactNode }) {
  return <div className={styles["actions"]}>{children}</div>;
}
