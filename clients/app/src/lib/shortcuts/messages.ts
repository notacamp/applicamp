import {
  shortcutMessages as designSystemMessages,
  type ShortcutMessages as DesignSystemShortcutMessages,
} from "@notacamp/ui/shortcuts";

import { mergeMessages, type Locale, type Messages } from "~/lib/i18n";

import en from "./i18n/en.json";
import es from "./i18n/es-ES.json";
import fr from "./i18n/fr.json";
import pt from "./i18n/pt-PT.json";

/**
 * The `shortcuts` i18n namespace (labels and group names): the design
 * system's, then Applicamp's labels over them.
 */
export const shortcutMessages: Record<Locale, Messages> = mergeMessages(designSystemMessages, {
  en,
  "pt-PT": pt,
  "es-ES": es,
  fr,
});

/** The `shortcuts` namespace's keys, for typing `t()`. */
export type ShortcutMessages = DesignSystemShortcutMessages & typeof en;
