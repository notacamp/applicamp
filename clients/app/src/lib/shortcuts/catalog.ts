import { UI_SHORTCUTS, type ShortcutDefinition } from "@notacamp/ui/shortcuts";

/**
 * Applicamp's shortcuts, declared once: the design system's own (lists, ⌘K,
 * `?`, Escape, undo) plus going to each place (G, then a letter). Screens
 * bind handlers to these ids with `useShortcut`; the `?` sheet, the ⌘K menu
 * and every hint read the keys from here, so a hint never drifts from its
 * binding.
 */
export const APP_SHORTCUTS = [
  {
    id: "nav.today",
    keys: "g t",
    scope: "global",
    group: "navigation",
    label: "shortcuts:nav.today",
  },
  {
    id: "nav.pipeline",
    keys: "g p",
    scope: "global",
    group: "navigation",
    label: "shortcuts:nav.pipeline",
  },
  {
    id: "nav.agents",
    keys: "g a",
    scope: "global",
    group: "navigation",
    label: "shortcuts:nav.agents",
  },
  {
    id: "nav.profile",
    keys: "g y",
    scope: "global",
    group: "navigation",
    label: "shortcuts:nav.profile",
  },
] as const satisfies readonly ShortcutDefinition[];

/** Every shortcut the app's registry holds. */
export const DEFAULT_SHORTCUTS: readonly ShortcutDefinition[] = [...UI_SHORTCUTS, ...APP_SHORTCUTS];

/** A shortcut of the app's own catalogue. */
export type AppShortcutId = (typeof APP_SHORTCUTS)[number]["id"];
