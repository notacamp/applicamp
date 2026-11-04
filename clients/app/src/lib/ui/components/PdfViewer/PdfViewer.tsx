import { Button, ButtonLink, useFocusTrap } from "@notacamp/ui";
import { useRef } from "react";

import { useShortcut } from "~/lib/shortcuts";

import { PdfPages } from "../PdfPages/PdfPages";
import styles from "./PdfViewer.module.css";

export interface PdfViewerLabels {
  close: string;
  download: string;
  /** "Loading the PDF…" */
  loading: string;
  /** Plain words for a PDF that would not open. */
  failed: string;
  /** Names a page's canvas ("Page 1 of 2"). */
  page: (page: number, pages: number) => string;
}

export interface PdfViewerProps {
  open: boolean;
  /** Close, Escape. */
  onClose: () => void;
  /** Where the PDF is, to view. */
  url: string;
  /** Where it is, to save (the download link). */
  downloadUrl: string;
  /** What it is ("Example Co · v2"). */
  title: string;
  /** One line under it (the date, the template, the pages). */
  subtitle?: string | undefined;
  labels: PdfViewerLabels;
  testId?: string | undefined;
}

/**
 * A PDF opened over the page: full screen on a phone, a large modal on a
 * desktop. Every page is drawn fitted to the width ({@link PdfPages}).
 * PDF.js is loaded when it is first opened. Escape or Close shut it;
 * Download is a plain link.
 */
export function PdfViewer({
  open,
  onClose,
  url,
  downloadUrl,
  title,
  subtitle,
  labels,
  testId = "pdf-viewer",
}: PdfViewerProps) {
  const sheet = useRef<HTMLDivElement>(null);
  useShortcut("overlay.close", onClose, { enabled: open });
  useFocusTrap(sheet, open);

  if (!open) {
    return null;
  }
  return (
    <div className={styles["scrim"]}>
      <div
        ref={sheet}
        className={styles["sheet"]}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        data-testid={testId}
      >
        <div className={styles["head"]}>
          <div className={styles["heading"]}>
            <div className={styles["title"]} data-testid={`${testId}-title`}>
              {title}
            </div>
            {subtitle === undefined ? null : (
              <div className={styles["subtitle"]} data-testid={`${testId}-subtitle`}>
                {subtitle}
              </div>
            )}
          </div>
          <div className={styles["actions"]}>
            <ButtonLink
              variant="secondary"
              icon="download"
              href={downloadUrl}
              data-testid={`${testId}-download`}
            >
              {labels.download}
            </ButtonLink>
            <Button
              variant="secondary"
              shortcut="overlay.close"
              data-autofocus=""
              data-testid={`${testId}-close`}
              onClick={onClose}
            >
              {labels.close}
            </Button>
          </div>
        </div>
        <div className={styles["body"]} data-testid={`${testId}-body`}>
          <PdfPages url={url} labels={labels} testId={testId} />
        </div>
      </div>
    </div>
  );
}
