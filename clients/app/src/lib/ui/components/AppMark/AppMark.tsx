import styles from "./AppMark.module.css";

export interface AppMarkProps {
  /** md: the sidebar's 20px square; lg: a page's. */
  size?: "md" | "lg" | undefined;
}

/** The product's mark: its initial in an ink square (decorative). */
export function AppMark({ size = "md" }: AppMarkProps) {
  return (
    <span className={styles["mark"]} data-size={size} aria-hidden="true">
      A
    </span>
  );
}

export interface AppBrandProps {
  /** The product's name. */
  name: string;
  /** A quiet line under it (who is signed in). */
  detail?: string | undefined;
  /** `sidebar`: padded to sit above the nav items. */
  place?: "page" | "sidebar" | undefined;
}

/** The mark and the name, for the sidebar's top and pages outside the app frame. */
export function AppBrand({ name, detail, place = "page" }: AppBrandProps) {
  return (
    <span className={styles["brand"]} data-place={place}>
      <AppMark />
      <span className={styles["text"]}>
        <span className={styles["name"]}>{name}</span>
        {detail === undefined ? null : <span className={styles["detail"]}>{detail}</span>}
      </span>
    </span>
  );
}
