import { useId, useState, type SubmitEvent } from "react";
import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import { Button, Dialog, Stack, TextField } from "~/lib/ui";

import { useAddOpportunity, type Opportunity } from "../../api/opportunities";

/** The longest link the server takes. */
export const MAX_URL_CHARS = 2000;

export interface NewOpportunityDialogProps {
  open: boolean;
  onClose: () => void;
  /** It is in the pipeline: open it (to type its role and company, for a link to another site). */
  onAdded: (opportunity: Opportunity) => void;
}

/** Whether `text` is an http(s) link the server can take. */
export function isLink(text: string): boolean {
  if (text.length > MAX_URL_CHARS) {
    return false;
  }
  try {
    const url = new URL(text);
    return url.protocol === "https:" || url.protocol === "http:";
  } catch {
    return false;
  }
}

/**
 * New: paste a job's link. A Greenhouse, Lever or Ashby posting is read for
 * you; for any other site the opportunity keeps the link and you type its
 * role and company.
 */
export function NewOpportunityDialog({ open, onClose, onAdded }: NewOpportunityDialogProps) {
  const { t } = useTranslation("pipeline");
  const formId = useId();
  const add = useAddOpportunity();
  const [url, setUrl] = useState("");
  const [invalid, setInvalid] = useState(false);

  const close = () => {
    setUrl("");
    setInvalid(false);
    add.reset();
    onClose();
  };

  const submit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    const link = url.trim();
    if (!isLink(link)) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    add.mutate(
      { url: link },
      {
        onSuccess: (opportunity) => {
          setUrl("");
          onAdded(opportunity);
        },
      },
    );
  };

  let error: string | undefined;
  if (invalid || problemCode(add.error) === "invalid_url") {
    error = t("new.invalid");
  } else if (add.isError) {
    error = t("new.failed");
  }

  return (
    <Dialog
      open={open}
      onClose={close}
      visor={null}
      title={t("new.title")}
      description={t("new.description")}
      footer={
        <>
          <Button variant="secondary" data-testid="new-opportunity-cancel" onClick={close}>
            {t("new.cancel")}
          </Button>
          <Button
            variant="primary"
            type="submit"
            form={formId}
            disabled={add.isPending}
            data-testid="new-opportunity-submit"
          >
            {add.isPending ? t("new.adding") : t("new.add")}
          </Button>
        </>
      }
    >
      <form id={formId} onSubmit={submit} noValidate>
        <Stack>
          <TextField
            label={t("new.url")}
            type="url"
            inputMode="url"
            autoComplete="off"
            placeholder="https://"
            value={url}
            error={error}
            onChange={(event) => {
              setUrl(event.target.value);
            }}
            data-testid="new-opportunity-url"
          />
        </Stack>
      </form>
    </Dialog>
  );
}
