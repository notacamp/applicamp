import type { Locale, Messages } from "~/lib/i18n";
import { UI_NAMESPACES } from "~/lib/ui";

import identityEn from "~/modules/identity/i18n/en.json";
import identityEs from "~/modules/identity/i18n/es-ES.json";
import identityFr from "~/modules/identity/i18n/fr.json";
import identityPt from "~/modules/identity/i18n/pt-PT.json";

import en from "./i18n/en.json";
import es from "./i18n/es-ES.json";
import fr from "./i18n/fr.json";
import pt from "./i18n/pt-PT.json";

/**
 * Every i18n namespace in every language: the shell's, each module's (named
 * after it) and the design system's (`ui`, `shortcuts`). All bundled: the app
 * is small, so nothing waits on its strings.
 */
export const NAMESPACES: Record<string, Record<Locale, Messages>> = {
  shell: { en, "pt-PT": pt, "es-ES": es, fr },
  identity: { en: identityEn, "pt-PT": identityPt, "es-ES": identityEs, fr: identityFr },
  ...UI_NAMESPACES,
};
