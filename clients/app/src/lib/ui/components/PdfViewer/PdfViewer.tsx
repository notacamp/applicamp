import { Button, ButtonLink, useFocusTrap } from "@notacamp/ui";
import type * as PdfjsModule from "pdfjs-dist";
import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
import { useEffect, useRef, useState } from "react";

import { useShortcut } from "~/lib/shortcuts";

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

type Pdfjs = typeof PdfjsModule;
type Task = PDFDocumentLoadingTask;
type Document = PDFDocumentProxy;

let loaded: Promise<Pdfjs> | undefined;

/**
 * PDF.js and its worker, fetched the first time a PDF is opened and never
 * before: from this app's own bundle (the worker is a file of the build),
 * nothing from a CDN.
 */
function loadPdfjs(): Promise<Pdfjs> {
  loaded ??= Promise.all([
    import("pdfjs-dist"),
    import("pdfjs-dist/build/pdf.worker.min.mjs?url"),
  ]).then(([pdfjs, worker]) => {
    pdfjs.GlobalWorkerOptions.workerSrc = worker.default;
    return pdfjs;
  });
  loaded.catch(() => {
    loaded = undefined;
  });
  return loaded;
}

/** What became of fetching `url`; a state of another url is a PDF still loading. */
type State =
  | { status: "failed"; url: string }
  | { status: "ready"; url: string; pages: number; document: Document };

/** The width, in CSS pixels, a page is drawn at: the body's, less its padding. */
function fitWidth(body: HTMLElement): number {
  const style = getComputedStyle(body);
  const padding = Number.parseFloat(style.paddingLeft) + Number.parseFloat(style.paddingRight);
  return Math.max(200, body.clientWidth - padding);
}

/**
 * A PDF opened over the page: full screen on a phone, a large modal on a
 * desktop. Every page is drawn on a canvas at the screen's pixel ratio and
 * fitted to the width (a phone's own PDF viewer often shows only the first
 * page). PDF.js is loaded when it is first opened. Escape or Close shut it;
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
  const body = useRef<HTMLDivElement>(null);
  const canvases = useRef(new Map<number, HTMLCanvasElement>());
  const [loadedState, setState] = useState<State | null>(null);
  const state = loadedState?.url === url ? loadedState : null;
  const [width, setWidth] = useState(0);
  useShortcut("overlay.close", onClose, { enabled: open });
  useFocusTrap(sheet, open);

  // Fetch the PDF and open it with PDF.js.
  useEffect(() => {
    if (!open) {
      return;
    }
    const controller = new AbortController();
    let task: Task | undefined;
    void (async () => {
      try {
        const [response, pdfjs] = await Promise.all([
          fetch(url, { credentials: "same-origin", signal: controller.signal }),
          loadPdfjs(),
        ]);
        if (!response.ok) {
          throw new Error(`The PDF answered ${String(response.status)}`);
        }
        const data = new Uint8Array(await response.arrayBuffer());
        task = pdfjs.getDocument({ data });
        const document = await task.promise;
        if (controller.signal.aborted) {
          return;
        }
        setState({ status: "ready", url, pages: document.numPages, document });
      } catch {
        if (!controller.signal.aborted) {
          setState({ status: "failed", url });
        }
      }
    })();
    return () => {
      controller.abort();
      void task?.destroy();
    };
  }, [open, url]);

  // Follow the width the pages are drawn at.
  const ready = state?.status === "ready";
  useEffect(() => {
    const element = body.current;
    if (!ready || element === null) {
      return;
    }
    const measure = () => {
      setWidth(fitWidth(element));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [ready]);

  // Draw every page, fitted to the width, at the device's pixel ratio.
  useEffect(() => {
    if (state?.status !== "ready" || width === 0) {
      return;
    }
    const { document, pages } = state;
    const ratio = globalThis.devicePixelRatio || 1;
    const tasks: { cancel: () => void }[] = [];
    const run = { stopped: false };
    void (async () => {
      for (let number = 1; number <= pages; number += 1) {
        const canvas = canvases.current.get(number);
        if (canvas === undefined || run.stopped) {
          continue;
        }
        const page = await document.getPage(number);
        const natural = page.getViewport({ scale: 1 });
        const scale = width / natural.width;
        const viewport = page.getViewport({ scale: scale * ratio });
        canvas.width = Math.floor(viewport.width);
        canvas.height = Math.floor(viewport.height);
        canvas.style.width = `${String(width)}px`;
        canvas.style.height = `${String(Math.floor(viewport.height / ratio))}px`;
        const task = page.render({ canvas, viewport });
        tasks.push(task);
        try {
          await task.promise;
          canvas.dataset["drawn"] = "";
        } catch {
          // Cancelled by a new width: the next pass draws it.
        }
      }
    })();
    return () => {
      run.stopped = true;
      for (const task of tasks) {
        task.cancel();
      }
    };
  }, [state, width]);

  if (!open) {
    return null;
  }
  const pages = state?.status === "ready" ? state.pages : 0;
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
        <div ref={body} className={styles["body"]} data-testid={`${testId}-body`}>
          {state === null ? (
            <p className={styles["note"]} role="status" data-testid={`${testId}-loading`}>
              {labels.loading}
            </p>
          ) : null}
          {state?.status === "failed" ? (
            <p className={styles["note"]} role="alert" data-testid={`${testId}-error`}>
              {labels.failed}
            </p>
          ) : null}
          {Array.from({ length: pages }, (_, index) => index + 1).map((number) => (
            <canvas
              key={`${url}#${String(number)}`}
              ref={(element) => {
                if (element === null) {
                  canvases.current.delete(number);
                } else {
                  canvases.current.set(number, element);
                }
              }}
              className={styles["page"]}
              aria-label={labels.page(number, pages)}
              data-testid={`${testId}-page-${String(number)}`}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
