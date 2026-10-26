// The shortcut system. The engine (one registry of definitions and bound
// handlers feeding the key listener, the `?` sheet, the ⌘K menu and every
// keyboard hint) is @notacamp/ui's; Applicamp's catalogue and labels live here.
export {
  ShortcutProvider,
  ShortcutRegistry,
  useShortcut,
  useShortcuts,
  type ShortcutDefinition,
} from "@notacamp/ui/shortcuts";
export { APP_SHORTCUTS, DEFAULT_SHORTCUTS, type AppShortcutId } from "./catalog";
export { shortcutMessages, type ShortcutMessages } from "./messages";
