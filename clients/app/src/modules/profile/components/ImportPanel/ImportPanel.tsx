import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { problemCode } from "~/lib/api";
import {
  Banner,
  Button,
  ChoiceCards,
  Cluster,
  FileField,
  NoteField,
  Section,
  Stack,
  Tagged,
} from "~/lib/ui";

import {
  importQueryKey,
  MAX_FILE_BYTES,
  MAX_TEXT_CHARS,
  useCreateImport,
  type Import,
  type ImportSource,
} from "../../api/imports";
import type { ImportKind } from "../../api/profile";

/** The problems an import can answer with, each with its own message. */
export const IMPORT_ERRORS = [
  "invalid_upload",
  "file_too_large",
  "text_too_long",
  "not_a_zip",
  "archive_too_many_entries",
  "archive_entry_too_large",
  "archive_too_large",
  "archive_unsafe_path",
  "not_a_pdf",
  "pdf_unreadable",
  "pdf_too_many_pages",
  "pdf_no_text",
  "nothing_found",
  "ai_unavailable",
  "ai_daily_limit_reached",
  "ai_failed",
] as const;
export type ImportError = (typeof IMPORT_ERRORS)[number] | "generic";

function isImportError(code: string | undefined): code is (typeof IMPORT_ERRORS)[number] {
  return IMPORT_ERRORS.some((known) => known === code);
}

/** What to say about a failed upload: its problem's own message, else a generic one. */
export function importErrorOf(error: unknown): ImportError {
  const code = problemCode(error);
  return isImportError(code) ? code : "generic";
}

const ACCEPT: Record<"linkedin_zip" | "pdf", string> = {
  linkedin_zip: ".zip,application/zip",
  pdf: ".pdf,application/pdf",
};

export interface ImportPanelProps {
  /** What this instance can import (the LinkedIn export always). */
  kinds: readonly ImportKind[];
  /** The draft is ready to review. */
  onDrafted: (draft: Import) => void;
  /** Back to the profile; none when there is no profile to go back to. */
  onCancel?: (() => void) | undefined;
}

/**
 * The three ways in: the LinkedIn export (read on the server, no AI), a PDF
 * or pasted text (read by the AI model, when one is set up). Uploading makes
 * a draft; nothing is saved yet.
 */
export function ImportPanel({ kinds, onDrafted, onCancel }: ImportPanelProps) {
  const { t } = useTranslation("profile");
  const queryClient = useQueryClient();
  const create = useCreateImport();
  const [kind, setKind] = useState<ImportKind>("linkedin_zip");
  const [file, setFile] = useState<File | null>(null);
  const [text, setText] = useState("");
  const [local, setLocal] = useState<ImportError | null>(null);

  const choose = (next: ImportKind) => {
    setKind(next);
    setFile(null);
    setLocal(null);
    create.reset();
  };

  const submit = () => {
    setLocal(null);
    let source: ImportSource;
    if (kind === "text") {
      if (text.trim() === "") {
        setLocal("invalid_upload");
        return;
      }
      source = { kind, text };
    } else {
      if (file === null) {
        setLocal("invalid_upload");
        return;
      }
      if (file.size > MAX_FILE_BYTES[kind]) {
        setLocal("file_too_large");
        return;
      }
      source = { kind, file };
    }
    create.mutate(source, {
      onSuccess: (draft) => {
        queryClient.setQueryData(importQueryKey(draft.id), draft);
        onDrafted(draft);
      },
    });
  };

  const failed: ImportError | null = create.isError ? importErrorOf(create.error) : null;
  const error = local ?? failed;
  const options = kinds.map((value) => ({
    value,
    title: t(`import.kinds.${value}.title`),
    description: t(`import.kinds.${value}.description`),
  }));

  return (
    <Section title={t("import.title")} description={t("import.lead")} testId="import-panel">
      <Stack>
        {options.length > 1 ? (
          <Tagged testId="import-kinds">
            <ChoiceCards
              label={t("import.choose")}
              options={options}
              value={kind}
              onChange={choose}
            />
          </Tagged>
        ) : (
          <Tagged testId="import-zip-only">
            <Banner icon="document" title={t("import.zipOnlyTitle")}>
              {t("import.zipOnly")}
            </Banner>
          </Tagged>
        )}
        {kind === "text" ? (
          <NoteField
            label={t("import.textLabel")}
            placeholder={t("import.textPlaceholder")}
            value={text}
            onChange={setText}
            maxLength={MAX_TEXT_CHARS}
            counter={(left) => t("import.charactersLeft", { left })}
            onSubmit={submit}
            onCancel={() => {
              onCancel?.();
            }}
            disabled={create.isPending}
          />
        ) : (
          <FileField
            label={t(kind === "pdf" ? "import.pdfLabel" : "import.zipLabel")}
            hint={t(kind === "pdf" ? "import.pdfHint" : "import.zipHint")}
            accept={ACCEPT[kind]}
            file={file}
            onChange={(next) => {
              setFile(next);
              setLocal(null);
            }}
            chooseLabel={t("import.chooseFile")}
            disabled={create.isPending}
            testId="import-file"
          />
        )}
        {error === null ? null : (
          <Tagged testId="import-error">
            <Banner tone="danger" icon="warning" role="alert" title={t(`import.errors.${error}`)} />
          </Tagged>
        )}
        <Cluster>
          <Button
            variant="primary"
            icon="arrow-right"
            disabled={create.isPending}
            data-testid="import-submit"
            onClick={submit}
          >
            {create.isPending ? t("import.reading") : t("import.submit")}
          </Button>
          {onCancel === undefined ? null : (
            <Button variant="ghost" data-testid="import-cancel" onClick={onCancel}>
              {t("import.cancel")}
            </Button>
          )}
        </Cluster>
      </Stack>
    </Section>
  );
}
