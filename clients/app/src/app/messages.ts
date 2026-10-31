import type { Locale, Messages } from "~/lib/i18n";
import { UI_NAMESPACES } from "~/lib/ui";

import identityEn from "~/modules/identity/i18n/en.json";
import identityEs from "~/modules/identity/i18n/es-ES.json";
import identityFr from "~/modules/identity/i18n/fr.json";
import identityPt from "~/modules/identity/i18n/pt-PT.json";
import pipelineEn from "~/modules/pipeline/i18n/en.json";
import pipelineEs from "~/modules/pipeline/i18n/es-ES.json";
import pipelineFr from "~/modules/pipeline/i18n/fr.json";
import pipelinePt from "~/modules/pipeline/i18n/pt-PT.json";
import profileEn from "~/modules/profile/i18n/en.json";
import profileEs from "~/modules/profile/i18n/es-ES.json";
import profileFr from "~/modules/profile/i18n/fr.json";
import profilePt from "~/modules/profile/i18n/pt-PT.json";
import runsEn from "~/modules/runs/i18n/en.json";
import runsEs from "~/modules/runs/i18n/es-ES.json";
import runsFr from "~/modules/runs/i18n/fr.json";
import runsPt from "~/modules/runs/i18n/pt-PT.json";
import sourcesEn from "~/modules/sources/i18n/en.json";
import sourcesEs from "~/modules/sources/i18n/es-ES.json";
import sourcesFr from "~/modules/sources/i18n/fr.json";
import sourcesPt from "~/modules/sources/i18n/pt-PT.json";

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
  profile: { en: profileEn, "pt-PT": profilePt, "es-ES": profileEs, fr: profileFr },
  pipeline: { en: pipelineEn, "pt-PT": pipelinePt, "es-ES": pipelineEs, fr: pipelineFr },
  sources: { en: sourcesEn, "pt-PT": sourcesPt, "es-ES": sourcesEs, fr: sourcesFr },
  runs: { en: runsEn, "pt-PT": runsPt, "es-ES": runsEs, fr: runsFr },
  ...UI_NAMESPACES,
};
