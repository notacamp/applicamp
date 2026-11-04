import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, Dialog, Stack, Tagged, Text, TextArea } from "~/lib/ui";

import { usePasteSite } from "../../api/brief";
import { cvErrorCode, type Cv, type CvDraft } from "../../api/cv";
import { formatCost } from "../../api/labels";

/** Longest text the server reads from a paste. */
const MAX_PASTE = 20_000;

/**
 * When the company's site could not be read and the CV was written from the
 * posting and the profile only: says so, and offers "Paste what the site
 * says" (read like one of its pages, one AI call, its cost on the line under
 * the button).
 */
export function SiteNote({ id, cv, draft }: { id: string; cv: Cv; draft: CvDraft }) {
  const { t, i18n } = useTranslation("pipeline");
  const paste = usePasteSite(id);
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const failure = cv.company_read_failure ?? undefined;
  if (paste.isSuccess) {
    return (
      <Tagged testId="cv-site-done">
        <Text size="sm">{t("cv.site.done")}</Text>
      </Tagged>
    );
  }
  if (failure === undefined || draft.company_facts > 0 || draft.status !== "draft") {
    return null;
  }
  const cost = formatCost(cv.read_eur, i18n.language);
  return (
    <Tagged testId="cv-site">
      <Banner
        icon="warning"
        title={failure === "robots_disallows" ? t("cv.site.robots") : t("cv.site.unread")}
      >
        <Stack gap="sm">
          <Button
            variant="secondary"
            data-testid="cv-site-paste"
            onClick={() => {
              paste.reset();
              setOpen(true);
            }}
          >
            {t("cv.site.paste")}
          </Button>
          <Text tone="secondary" size="sm">
            {t("cv.site.cost", { cost })}
          </Text>
        </Stack>
      </Banner>
      <Dialog
        open={open}
        onClose={() => {
          setOpen(false);
        }}
        visor={null}
        title={t("cv.site.dialogTitle")}
        description={t("cv.site.hint")}
        footer={
          <>
            <Button
              variant="secondary"
              data-testid="cv-site-cancel"
              onClick={() => {
                setOpen(false);
              }}
            >
              {t("cv.site.cancel")}
            </Button>
            <Button
              variant="primary"
              ai
              disabled={paste.isPending || text.trim() === ""}
              data-testid="cv-site-read"
              onClick={() => {
                paste.mutate(text.trim(), {
                  onSuccess: () => {
                    setOpen(false);
                  },
                });
              }}
            >
              {paste.isPending ? t("cv.site.reading") : t("cv.site.read")}
            </Button>
          </>
        }
      >
        <Stack gap="sm">
          <TextArea
            label={t("cv.site.label")}
            rows={8}
            maxLength={MAX_PASTE}
            value={text}
            onChange={(event) => {
              setText(event.target.value);
            }}
            data-testid="cv-site-text"
          />
          <Text tone="secondary" size="sm">
            {t("cv.site.cost", { cost })}
          </Text>
          {paste.isError ? (
            <Tagged testId="cv-site-error">
              <Banner
                tone="danger"
                icon="warning"
                role="alert"
                title={t(`cv.errors.${cvErrorCode(paste.error)}`, { count: 1 })}
              />
            </Tagged>
          ) : null}
        </Stack>
      </Dialog>
    </Tagged>
  );
}
