import i18next, { type i18n } from "i18next";
import { initReactI18next } from "react-i18next";

import { DEFAULT_LOCALE, SUPPORTED_LOCALES, type Locale, type Messages } from "./locales";

export { formatDay } from "./dates";
export {
  DEFAULT_LOCALE,
  detectLocale,
  isLocale,
  SUPPORTED_LOCALES,
  type Locale,
  type Messages,
} from "./locales";

/** The app shell's namespace, the default one. */
export const SHELL_NAMESPACE = "shell";

export interface I18nSetup {
  locale: Locale;
  /** Every namespace, for every supported locale. The app is small: all bundled. */
  namespaces: Record<string, Record<Locale, Messages>>;
}

/**
 * Creates the i18next instance, synchronously (every namespace is bundled,
 * so nothing ever suspends on its strings).
 */
export function createI18n({ locale, namespaces }: I18nSetup): i18n {
  const instance = i18next.createInstance();
  void instance.use(initReactI18next).init({
    lng: locale,
    fallbackLng: DEFAULT_LOCALE,
    supportedLngs: SUPPORTED_LOCALES,
    // Locales are full tags (`pt-PT`); never fall back to a bare `pt`.
    load: "currentOnly",
    ns: Object.keys(namespaces),
    defaultNS: SHELL_NAMESPACE,
    resources: Object.fromEntries(
      SUPPORTED_LOCALES.map((code) => [
        code,
        Object.fromEntries(
          Object.entries(namespaces).map(([namespace, messages]) => [namespace, messages[code]]),
        ),
      ]),
    ),
    initAsync: false,
    // React already escapes rendered strings.
    interpolation: { escapeValue: false },
  });
  return instance;
}

/** Each language in its own name: the same in every locale. */
export const LANGUAGE_NAMES: Record<Locale, string> = {
  en: "English",
  "pt-PT": "Português (Portugal)",
  "es-ES": "Español (España)",
  fr: "Français",
};

/** `messages` per locale, each merged over `base`'s (the package's strings, then ours). */
export function mergeMessages(
  base: Record<Locale, Messages>,
  messages: Record<Locale, Messages>,
): Record<Locale, Messages> {
  return Object.fromEntries(
    SUPPORTED_LOCALES.map((locale) => [locale, deepMerge(base[locale], messages[locale])]),
  ) as Record<Locale, Messages>;
}

function isMessages(value: unknown): value is Messages {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function deepMerge(base: Messages, over: Messages): Messages {
  const merged: Messages = { ...base };
  for (const [key, value] of Object.entries(over)) {
    const current = merged[key];
    merged[key] = isMessages(current) && isMessages(value) ? deepMerge(current, value) : value;
  }
  return merged;
}
