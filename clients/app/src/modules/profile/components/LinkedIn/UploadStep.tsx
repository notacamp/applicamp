import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Banner, Button, ChoiceCards, Cluster, FileField, Stack, Tagged, Text } from "~/lib/ui";

import {
  MAX_LINKEDIN_BYTES,
  useCheckLinkedIn,
  type LinkedInKind,
  type LinkedInView,
} from "../../api/linkedin";
import { errorCode, type LinkedInErrorCode } from "./labels";

const KINDS: readonly LinkedInKind[] = ["linkedin_zip", "pdf"];

const ACCEPT: Record<LinkedInKind, string> = {
  linkedin_zip: ".zip,application/zip",
  pdf: ".pdf,application/pdf",
};

export interface UploadStepProps {
  view: LinkedInView;
  /** The review was made. */
  onDone: () => void;
  /** Back to the review there is; none when there is none. */
  onCancel?: (() => void) | undefined;
}

/**
 * The file: the LinkedIn data export (read on the server, no AI) or the
 * profile saved as PDF (read by the AI model). Uploading makes the review;
 * the file itself is not kept.
 */
export function UploadStep({ view, onDone, onCancel }: UploadStepProps) {
  const { t, i18n } = useTranslation("profile");
  const check = useCheckLinkedIn(i18n.language);
  const [kind, setKind] = useState<LinkedInKind>("linkedin_zip");
  const [file, setFile] = useState<File | null>(null);
  const [local, setLocal] = useState<LinkedInErrorCode | null>(null);
  const capped = view.checkups_today >= view.daily_cap;

  const submit = () => {
    setLocal(null);
    if (file === null) {
      setLocal("invalid_upload");
      return;
    }
    if (file.size > MAX_LINKEDIN_BYTES[kind]) {
      setLocal("file_too_large");
      return;
    }
    check.mutate({ kind, file }, { onSuccess: onDone });
  };

  const error = local ?? (check.isError ? errorCode(check.error) : null);
  const options = KINDS.map((value) => ({
    value,
    title: t(`linkedin.upload.kinds.${value}.title`),
    description: t(`linkedin.upload.kinds.${value}.description`),
  }));

  return (
    <Stack gap="lg" testId="linkedin-upload">
      <Text tone="secondary">{t("linkedin.upload.lead")}</Text>
      <Tagged testId="linkedin-kinds">
        <ChoiceCards
          label={t("linkedin.upload.choose")}
          options={options}
          value={kind}
          onChange={(next) => {
            setKind(next);
            setFile(null);
            setLocal(null);
            check.reset();
          }}
        />
      </Tagged>
      <FileField
        label={t(kind === "pdf" ? "linkedin.upload.pdfLabel" : "linkedin.upload.zipLabel")}
        hint={t(kind === "pdf" ? "linkedin.upload.pdfHint" : "linkedin.upload.zipHint")}
        accept={ACCEPT[kind]}
        file={file}
        onChange={(next) => {
          setFile(next);
          setLocal(null);
        }}
        chooseLabel={t("linkedin.upload.chooseFile")}
        disabled={check.isPending || capped}
        testId="linkedin-file"
      />
      {capped ? (
        <Tagged testId="linkedin-capped">
          <Banner icon="clock" title={t("linkedin.upload.capReached", { cap: view.daily_cap })} />
        </Tagged>
      ) : null}
      {error === null ? null : (
        <Tagged testId="linkedin-error">
          <Banner tone="danger" icon="warning" role="alert" title={t(`linkedin.errors.${error}`)} />
        </Tagged>
      )}
      <Cluster>
        <Button
          variant="primary"
          ai
          disabled={check.isPending || capped}
          data-testid="linkedin-submit"
          onClick={submit}
        >
          {check.isPending ? t("linkedin.upload.reading") : t("linkedin.upload.submit")}
        </Button>
        {onCancel === undefined ? null : (
          <Button
            variant="ghost"
            disabled={check.isPending}
            data-testid="linkedin-cancel"
            onClick={onCancel}
          >
            {t("linkedin.upload.cancel")}
          </Button>
        )}
      </Cluster>
    </Stack>
  );
}
