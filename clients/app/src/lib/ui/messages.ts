import { UI_NAMESPACES as DESIGN_SYSTEM_NAMESPACES } from "@notacamp/ui";

import { mergeMessages, type Locale, type Messages } from "~/lib/i18n";
import { shortcutMessages } from "~/lib/shortcuts";

import en from "./i18n/en.json";
import es from "./i18n/es-ES.json";
import fr from "./i18n/fr.json";
import pt from "./i18n/pt-PT.json";

/**
 * The `ui` namespace: the design system's strings, with Applicamp's in place
 * of those that name Campbooks (the command menu searches and runs, it does
 * not ask an assistant).
 */
const uiMessages: Record<Locale, Messages> = mergeMessages(DESIGN_SYSTEM_NAMESPACES.ui, {
  en,
  "pt-PT": pt,
  "es-ES": es,
  fr,
});

/**
 * Namespaces the design system needs on first render (`ui`, `shortcuts`).
 * Give them to `createI18n` with the app's own.
 */
export const UI_NAMESPACES = { ui: uiMessages, shortcuts: shortcutMessages } as const;
