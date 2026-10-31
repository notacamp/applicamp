import { useTranslation } from "react-i18next";

import { Cluster, Select, Stack, Tagged, Text, ViewSwitch } from "~/lib/ui";

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
 * The look of the PDF (template, pages) and the CV's language. The first two
 * make a PDF from the same text, with no AI; a different language is a new
 * draft, which the note says.
 */
export function LayoutControls({
  layout,
  onLayout,
  language,
  onLanguage,
  disabled,
}: {
  layout: CvLayout;
  onLayout: (layout: CvLayout) => void;
  language: string;
  onLanguage: (language: string) => void;
  /** Something is running. */
  disabled: boolean;
}) {
  const { t, i18n } = useTranslation("pipeline");
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
          <Select
            label={t("cv.layout.language")}
            options={CV_LANGUAGES.map((tag) => ({
              value: tag,
              label: languageName(tag, i18n.language),
            }))}
            value={cvLanguage(language)}
            onChange={onLanguage}
            data-testid="cv-language-select"
          />
        </Cluster>
        <Text tone="secondary" size="sm">
          {t("cv.layout.note")}
        </Text>
      </Stack>
    </Tagged>
  );
}
