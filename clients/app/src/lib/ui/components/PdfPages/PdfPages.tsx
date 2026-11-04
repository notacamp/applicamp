import type * as PdfjsModule from "pdfjs-dist";
import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
import { useEffect, useRef, useState } from "react";

import styles from "./PdfPages.module.css";

export interface PdfPagesLabels {
  /** "Loading the PDF…" */
  loading: string;
  /** Plain words for a PDF that would not open. */
  failed: string;
  /** Names a page's canvas ("Page 1 of 2"). */
  page: (page: number, pages: number) => string;
}

export interface PdfPagesProps {
  /** Where the PDF is. A new address loads it again. */
  url: string;
  labels: PdfPagesLabels;
  /** The prefix of the test ids: `<testId>-page-1`, `<testId>-loading`, `<testId>-error`. */
  testId: string;
  /** Load it: off while the PDF is not to be shown. */
  enabled?: boolean | undefined;
}

type Pdfjs = typeof PdfjsModule;
type Task = PDFDocumentLoadingTask;
type Document = PDFDocumentProxy;

let loaded: Promise<Pdfjs> | undefined;

/**
 * PDF.js and its worker, fetched the first time a PDF is shown and never
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

/**
 * Every page of a PDF, drawn on canvases at the screen's pixel ratio and
 * fitted to the width it is given (a phone's own PDF viewer often shows only
 * the first page). PDF.js is loaded when a PDF is first shown. A drawn page
 * carries `data-drawn`; while a new address loads, the pages already drawn
 * stay.
 */
export function PdfPages({ url, labels, testId, enabled = true }: PdfPagesProps) {
  const root = useRef<HTMLDivElement>(null);
  const canvases = useRef(new Map<number, HTMLCanvasElement>());
  const [loadedState, setState] = useState<State | null>(null);
  // While a new address loads, the pages already drawn stay: no flash.
  const state = loadedState?.url === url || loadedState?.status === "ready" ? loadedState : null;
  const [width, setWidth] = useState(0);

  // Fetch the PDF and open it with PDF.js.
  useEffect(() => {
    if (!enabled) {
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
  }, [enabled, url]);

  // Follow the width the pages are drawn at.
  const ready = state?.status === "ready";
  useEffect(() => {
    const element = root.current;
    if (!ready || element === null) {
      return;
    }
    const measure = () => {
      setWidth(Math.max(200, element.clientWidth));
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

  const pages = state?.status === "ready" ? state.pages : 0;
  return (
    <div ref={root} className={styles["pages"]} data-testid={`${testId}-pages`}>
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
          key={`${state?.url ?? url}#${String(number)}`}
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
  );
}
