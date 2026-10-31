import type { ReactNode } from "react";

import styles from "./Cv.module.css";

export interface ReviewLayoutProps {
  /** The card beside the CV (the check, the keywords); above it on narrow pages. */
  aside: ReactNode;
  /** The CV. */
  children: ReactNode;
  /** For tests: `data-testid` of the layout. */
  testId?: string | undefined;
}

/**
 * The CV and a card beside it. When the page is too narrow for both (a
 * phone, or a laptop beside the sidebar: it asks its own width, not the
 * screen's) they stack, the card first, so what needs the person is seen
 * without scrolling past the whole CV.
 */
export function ReviewLayout({ aside, children, testId }: ReviewLayoutProps) {
  return (
    <div className={styles["frame"]}>
      <div className={styles["review"]} data-testid={testId}>
        <aside className={styles["aside"]}>{aside}</aside>
        <div className={styles["main"]}>{children}</div>
      </div>
    </div>
  );
}

/** The CV as a page of paper. */
export function Paper({
  label,
  children,
  testId,
}: {
  /** What the page is (its accessible name). */
  label: string;
  children: ReactNode;
  testId?: string | undefined;
}) {
  return (
    <article className={styles["paper"]} aria-label={label} data-testid={testId}>
      {children}
    </article>
  );
}

/** The name and the line under it, at the top of the page. */
export function PaperHead({
  name,
  headline,
  contact,
}: {
  name?: ReactNode;
  headline?: ReactNode;
  contact?: ReactNode;
}) {
  return (
    <header className={styles["head"]}>
      {name === undefined ? null : <p className={styles["name"]}>{name}</p>}
      {headline === undefined ? null : <div className={styles["headline"]}>{headline}</div>}
      {contact === undefined ? null : <p className={styles["contact"]}>{contact}</p>}
    </header>
  );
}

/** A part of the page (Summary, Experience, Skills…) under a small heading. */
export function PaperSection({
  title,
  children,
  testId,
}: {
  title: string;
  children: ReactNode;
  testId?: string | undefined;
}) {
  return (
    <section className={styles["section"]} aria-label={title} data-testid={testId}>
      <h2 className={styles["section-title"]} aria-hidden="true">
        {title}
      </h2>
      {children}
    </section>
  );
}

/** One position: its title and dates, and its lines. */
export function PaperRole({
  title,
  meta,
  children,
  testId,
}: {
  title: string;
  meta?: string | null | undefined;
  children: ReactNode;
  testId?: string | undefined;
}) {
  return (
    <div className={styles["role"]} data-testid={testId}>
      <div className={styles["role-head"]}>
        <h3 className={styles["role-title"]}>{title}</h3>
        {meta === undefined || meta === null ? null : (
          <span className={styles["role-meta"]}>{meta}</span>
        )}
      </div>
      {children}
    </div>
  );
}

/** The lines of a part of the page. */
export function CvLines({ children }: { children: ReactNode }) {
  return <ul className={styles["lines"]}>{children}</ul>;
}

/**
 * One line of the CV: its text, a mark of how it relates to the profile and
 * what can be done with it. A flagged one is picked out and carries its
 * reason and its actions.
 */
export function CvLine({
  flagged = false,
  mark,
  children,
  detail,
  testId,
  id,
}: {
  /** It cannot be traced to the profile. */
  flagged?: boolean | undefined;
  /** How it relates to the profile ("Reworded"), beside the text. */
  mark?: ReactNode;
  /** The line's text. */
  children: ReactNode;
  /** Under the text, full width: the sources, or the reason and the actions. */
  detail?: ReactNode;
  testId?: string | undefined;
  /** The line's address on the page, for "Go to the first one". */
  id?: string | undefined;
}) {
  return (
    <li
      id={id}
      className={styles["line"]}
      data-flagged={flagged ? "" : undefined}
      data-testid={testId}
    >
      <div className={styles["line-main"]}>
        <span className={styles["line-text"]}>{children}</span>
        {mark === undefined ? null : <span className={styles["line-mark"]}>{mark}</span>}
      </div>
      {detail === undefined ? null : <div className={styles["line-detail"]}>{detail}</div>}
    </li>
  );
}

/** A run of chips, wrapping (the skills). */
export function ChipList({ children }: { children: ReactNode }) {
  return <div className={styles["chips"]}>{children}</div>;
}

/** Text with some of its words marked (the claims a source says). */
export function Marked({ text, words }: { text: string; words: readonly string[] }) {
  const parts = markedParts(text, words);
  return (
    <>
      {parts.map((part, index) =>
        part.hit ? (
          // The text never changes while it is shown, so a part's place is its key.
          <mark key={`${String(index)}-${part.text}`} className={styles["marked"]}>
            {part.text}
          </mark>
        ) : (
          <span key={`${String(index)}-${part.text}`}>{part.text}</span>
        ),
      )}
    </>
  );
}

/** `text` cut into runs, the ones that are one of `words` (any case) marked. */
export function markedParts(
  text: string,
  words: readonly string[],
): { text: string; hit: boolean }[] {
  const wanted = new Set(words.map((word) => word.toLowerCase()).filter((word) => word !== ""));
  if (wanted.size === 0) {
    return [{ text, hit: false }];
  }
  const parts: { text: string; hit: boolean }[] = [];
  for (const piece of text.split(/([\p{L}\p{N}+#]+)/u)) {
    if (piece === "") {
      continue;
    }
    const hit = wanted.has(piece.toLowerCase());
    const last = parts.at(-1);
    if (last?.hit === hit) {
      last.text += piece;
    } else {
      parts.push({ text: piece, hit });
    }
  }
  return parts;
}
