import { createI18n, detectLocale } from "~/lib/i18n";
import { readPref } from "~/lib/storage/prefs";

import { NAMESPACES } from "./messages";

/** The app's i18next instance: the saved language, else the browser's. */
export const i18n = createI18n({
  locale: detectLocale(readPref("locale"), navigator.languages),
  namespaces: NAMESPACES,
});

const syncDocumentLanguage = (language: string) => {
  document.documentElement.lang = language;
};
syncDocumentLanguage(i18n.language);
i18n.on("languageChanged", syncDocumentLanguage);
