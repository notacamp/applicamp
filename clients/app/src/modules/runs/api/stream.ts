import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import type { Schema } from "~/lib/api";

import { settleRun, type Run } from "./runs";

/** A line a run's agent or its sandbox wrote. */
export type RunEvent = Schema<"RunEvent">;
export type Layer = Schema<"Layer">;
export type Level = Schema<"Level">;

/**
 * Where the live view is: `connecting` (or reconnecting, on its own),
 * `live`, `ended` once the run is over and every line is in, `lost` when
 * the server refused it and only Retry brings it back.
 */
export type StreamState = "connecting" | "live" | "ended" | "lost";

export interface RunEvents {
  /** Every line so far, both layers, oldest first. */
  events: RunEvent[];
  state: StreamState;
  /** After `lost`: follow it again from the last line seen. */
  retry: () => void;
}

/** The live stream of run `id`, after line `after`. */
export function streamUrl(id: string, after: number): string {
  const path = `/api/v1/runs/${encodeURIComponent(id)}/events/stream`;
  return after > 0 ? `${path}?after=${String(after)}` : path;
}

/**
 * What run `id` does, live (`GET /api/v1/runs/{id}/events/stream`): its
 * lines so far, then each as it is written, until the run ends. The browser
 * reconnects by itself with `Last-Event-ID`; the session cookie goes with
 * it (a GET needs no CSRF header). Mount it once per run (`key` the
 * component by the run's id): its state is that run's.
 */
export function useRunEvents(id: string): RunEvents {
  const queryClient = useQueryClient();
  const [events, setEvents] = useState<RunEvent[]>([]);
  const [state, setState] = useState<StreamState>("connecting");
  const [attempt, setAttempt] = useState(0);
  // The last line seen, so a retry resumes after it.
  const last = useRef(0);

  useEffect(() => {
    const source = new EventSource(streamUrl(id, last.current));
    const onOpen = () => {
      setState("live");
    };
    const onEvent = (message: MessageEvent<string>) => {
      const event = JSON.parse(message.data) as RunEvent;
      if (event.seq <= last.current) {
        return;
      }
      last.current = event.seq;
      setEvents((seen) => [...seen, event]);
    };
    const onEnd = (message: MessageEvent<string>) => {
      source.close();
      setState("ended");
      void settleRun(queryClient, JSON.parse(message.data) as Run);
    };
    const onError = () => {
      // CLOSED: the server answered with an error, and the browser gave up.
      setState(source.readyState === EventSource.CLOSED ? "lost" : "connecting");
    };
    source.addEventListener("open", onOpen);
    source.addEventListener("run_event", onEvent);
    source.addEventListener("end", onEnd);
    source.addEventListener("error", onError);
    return () => {
      source.removeEventListener("open", onOpen);
      source.removeEventListener("run_event", onEvent);
      source.removeEventListener("end", onEnd);
      source.removeEventListener("error", onError);
      source.close();
    };
  }, [id, attempt, queryClient]);

  return {
    events,
    state,
    retry: () => {
      setState("connecting");
      setAttempt((n) => n + 1);
    },
  };
}
