import styles from "./Bullets.module.css";

/** A short list of sentences with bullets (why a job matches, its gaps). */
export function Bullets({
  items,
  testId,
}: {
  items: readonly string[];
  testId?: string | undefined;
}) {
  return (
    <ul className={styles["list"]} data-testid={testId}>
      {items.map((item, index) => (
        // The same sentence can come twice; its place keeps it apart.
        <li key={`${String(index)}-${item}`} className={styles["item"]}>
          {item}
        </li>
      ))}
    </ul>
  );
}
