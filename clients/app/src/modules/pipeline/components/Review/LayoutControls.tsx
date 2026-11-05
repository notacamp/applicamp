import { useTranslation } from "react-i18next";

import { Cluster, Stack, Tagged, Text, ViewSwitch } from "~/lib/ui";

import { LENGTHS, TEMPLATES, type CvLayout } from "../../api/cv";
import { languageName } from "../../api/labels";

/** The languages a CV can be written in. */
export const CV_LANGUAGES = ["en", "pt-PT", "es-ES", "fr"] as const;

/** `tag` as the CV's own language, one of {@link CV_LANGUAGES} (`pt` is `pt-PT`). */
export function cvLanguage(tag: string): (typeof CV_LANGUAGES)[number] {
  const base = tag.split("-", 1)[0]?.toLowerCase();
  return CV_LANGUAGES.find((language) => language.toLowerCase().split("-", 1)[0] === base) ?? "en";
}

/**
 * The look of the PDF and the CV's language, as three labelled pairs: Style
 * (Clean, Compact), Length (1 page, 2 pages) and Language (the CV's and one
 * other). Style and length make the PDF again from the same text, with no AI;
 * another language is a new draft, written again with Write it again.
 */
export function LayoutControls({
  layout,
  onLayout,
  language,
  onLanguage,
  languages,
  disabled,
}: {
  layout: CvLayout;
  onLayout: (layout: CvLayout) => void;
  language: string;
  onLanguage: (language: string) => void;
  /** The languages to choose between: the CV's first. */
  languages: readonly string[];
  /** Something is running. */
  disabled: boolean;
}) {
  const { t, i18n } = useTranslation("pipeline");
  const chosen = cvLanguage(language);
  return (
    <Tagged testId="cv-layout-controls">
      <Stack gap="sm">
        <Cluster>
          <Text tone="secondary" size="sm">
            {t("cv.layout.template")}
          </Text>
          <ViewSwitch
            label={t("cv.layout.template")}
            options={TEMPLATES.map((template) => ({
              value: template,
              label: t(`cv.layout.templates.${template}`),
              testId: `cv-template-${template}`,
            }))}
            value={layout.template}
            onChange={(template) => {
              if (!disabled && template !== layout.template) {
                onLayout({ ...layout, template });
              }
            }}
          />
        </Cluster>
        <Cluster>
          <Text tone="secondary" size="sm">
            {t("cv.layout.length")}
          </Text>
          <ViewSwitch
            label={t("cv.layout.length")}
            options={LENGTHS.map((length) => ({
              value: String(length) as "1" | "2",
              label: t("cv.layout.pages", { count: length }),
              testId: `cv-length-${String(length)}`,
            }))}
            value={String(layout.length) as "1" | "2"}
            onChange={(value) => {
              const length = value === "2" ? 2 : 1;
              if (!disabled && length !== layout.length) {
                onLayout({ ...layout, length });
              }
            }}
          />
        </Cluster>
        <Cluster>
          <Text tone="secondary" size="sm">
            {t("cv.layout.language")}
          </Text>
          <ViewSwitch
            label={t("cv.layout.language")}
            options={languages.map((tag) => ({
              value: cvLanguage(tag),
              label: languageName(tag, i18n.language),
              testId: `cv-language-${cvLanguage(tag)}`,
            }))}
            value={chosen}
            onChange={(tag) => {
              if (!disabled) {
                onLanguage(tag);
              }
            }}
          />
        </Cluster>
        <Text tone="secondary" size="sm">
          {t("cv.layout.note")}
        </Text>
      </Stack>
    </Tagged>
  );
}
