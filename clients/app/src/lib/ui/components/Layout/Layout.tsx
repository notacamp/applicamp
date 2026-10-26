import { Heading } from "@notacamp/ui";
import { useId, type ReactNode } from "react";

import styles from "./Layout.module.css";

export interface StackProps {
  /** Space between the children: sm 8px, md 16px, lg 24px. */
  gap?: "sm" | "md" | "lg" | undefined;
  children: ReactNode;
  /** For tests: `data-testid` of the stack. */
  testId?: string | undefined;
}

/** Children one under the other, evenly spaced. */
export function Stack({ gap = "md", children, testId }: StackProps) {
  return (
    <div className={styles["stack"]} data-gap={gap} data-testid={testId}>
      {children}
    </div>
  );
}

/** Children side by side, wrapping onto more lines on a narrow screen (a row of buttons). */
export function Cluster({ children }: { children: ReactNode }) {
  return <div className={styles["cluster"]}>{children}</div>;
}

export interface SectionProps {
  /** The section's heading, an h2. */
  title: string;
  /** One sentence under it. */
  description?: ReactNode | undefined;
  /** Beside the heading (an Add button); under it on phones. */
  action?: ReactNode | undefined;
  children?: ReactNode;
  /** For tests: `data-testid` of the section. */
  testId?: string | undefined;
}

/** A part of a page: a heading, a sentence, an action and what it holds. */
export function Section({ title, description, action, children, testId }: SectionProps) {
  const id = useId();
  return (
    <section className={styles["section"]} aria-labelledby={id} data-testid={testId}>
      <div className={styles["head"]}>
        <Heading level={2} size="title" id={id}>
          {title}
        </Heading>
        {action === undefined ? null : <div className={styles["action"]}>{action}</div>}
      </div>
      {description === undefined ? null : <p className={styles["description"]}>{description}</p>}
      {children}
    </section>
  );
}
