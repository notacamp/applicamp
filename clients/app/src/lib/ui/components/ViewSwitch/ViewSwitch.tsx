import { Icon, type IconName } from "@notacamp/ui";

import styles from "./ViewSwitch.module.css";

export interface ViewSwitchOption<T extends string> {
  value: T;
  label: string;
  icon?: IconName | undefined;
  /** For tests: `data-testid` of its button. */
  testId?: string | undefined;
}

export interface ViewSwitchProps<T extends string> {
  /** Names the group ("View"). */
  label: string;
  options: readonly ViewSwitchOption<T>[];
  value: T;
  onChange: (value: T) => void;
}

/**
 * A database's views, like Notion's tabs over a table ("All opportunities",
 * "By stage"): one pressed, underlined. Each button can carry a test id,
 * which the package's Segmented cannot.
 */
export function ViewSwitch<T extends string>({
  label,
  options,
  value,
  onChange,
}: ViewSwitchProps<T>) {
  return (
    <div className={styles["views"]} role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          className={styles["view"]}
          aria-pressed={option.value === value}
          data-testid={option.testId}
          onClick={() => {
            onChange(option.value);
          }}
        >
          {option.icon === undefined ? null : <Icon name={option.icon} />}
          {option.label}
        </button>
      ))}
    </div>
  );
}
