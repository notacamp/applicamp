import { Heading, Icon, type IconName } from "@notacamp/ui";
import type { ReactNode } from "react";

import styles from "./Placeholder.module.css";

export interface PlaceholderProps {
  /** What is not here yet ("No opportunities yet"). */
  title: string;
  /** What will appear here, and how it gets here. */
  children?: ReactNode;
  icon: IconName;
  /** One way forward, usually a Button or ButtonLink. */
  action?: ReactNode;
  /** For tests: `data-testid` of the panel. */
  testId?: string | undefined;
}

/**
 * A place with nothing in it yet, saying what will appear there: an icon, a
 * title, a sentence or two and one action, in a quiet dashed panel. (The
 * package's EmptyState carries Campbooks' visor, which Applicamp has none of.)
 */
export function Placeholder({ title, children, icon, action, testId }: PlaceholderProps) {
  return (
    <section className={styles.placeholder} data-testid={testId}>
      <span className={styles.icon}>
        <Icon name={icon} size="lg" />
      </span>
      <Heading level={2} size="title">
        {title}
      </Heading>
      {children === undefined ? null : <div className={styles.body}>{children}</div>}
      {action}
    </section>
  );
}
