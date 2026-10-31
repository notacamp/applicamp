// Invented runs and run events for the client's tests, and a stand-in for
// the browser's EventSource (jsdom has none). No real host or person.
import { act } from "@testing-library/react";
import { vi } from "vitest";

import type { Schema } from "~/lib/api";

type Run = Schema<"Run">;
type RunEvent = Schema<"RunEvent">;
type RunList = Schema<"RunList">;

export const RUN_ID = "0190a1b2-0000-7000-8000-0000000000c1";
export const OLDER_RUN_ID = "0190a1b2-0000-7000-8000-0000000000c0";

/** A finished check, as the server sends it. */
export function run(overrides: Partial<Run> = {}): Run {
  return {
    id: RUN_ID,
    kind: "runs.probe",
    status: "finished",
    outcome: "succeeded",
    stopping: false,
    created_at: "2026-10-30T09:00:00Z",
    started_at: "2026-10-30T09:00:01Z",
    finished_at: "2026-10-30T09:00:05Z",
    deadline: null,
    duration_ms: 4200,
    sandbox: "run-0190a1b2c1",
    image: "ghcr.io/notacamp/applicamp-agent:main",
    policy: "policies/probe.yaml",
    model: null,
    tokens_in: 0,
    tokens_out: 0,
    cost_eur: 0,
    error_kind: null,
    summary: { checks: ["context", "tool", "cap", "network"] },
    ...overrides,
  };
}

/** The list of runs, and whether a check can start. */
export function runList(runs: Run[], overrides: Partial<RunList> = {}): RunList {
  return { runs, available: true, startable: ["runs.probe"], ...overrides };
}

/** A line of a run. */
export function runEvent(overrides: Partial<RunEvent> = {}): RunEvent {
  return {
    seq: 1,
    at: "2026-10-30T09:00:02Z",
    layer: "agent",
    level: "info",
    message: "Read its context.",
    operation: "context",
    tool_name: null,
    ...overrides,
  };
}

/** The probe's lines: its steps, and the proxy's allow and deny. */
export const PROBE_EVENTS: RunEvent[] = [
  runEvent({ seq: 1 }),
  runEvent({
    seq: 2,
    level: "tool",
    message: "Called probe_echo; it answered.",
    operation: null,
    tool_name: "probe_echo",
  }),
  runEvent({
    seq: 3,
    level: "deny",
    message: "probe_echo was refused past its cap of one call.",
    operation: null,
    tool_name: "probe_echo",
  }),
  runEvent({
    seq: 4,
    layer: "sandbox",
    level: "info",
    message:
      "NET:OPEN [INFO] ALLOWED applicamp-agent -> applicamp.example.com:443 [policy:runs.probe engine:fake]",
    operation: "NET:OPEN",
  }),
  runEvent({
    seq: 5,
    layer: "sandbox",
    level: "deny",
    message:
      "NET:OPEN [MED] DENIED applicamp-agent -> example.com:443 [policy:runs.probe engine:fake]",
    operation: "NET:OPEN",
  }),
  runEvent({
    seq: 6,
    level: "deny",
    message: "example.com is blocked: it is outside its policy.",
    operation: "network",
  }),
];

/**
 * The browser's EventSource, driven by the test: every one opened is in
 * `FakeEventSource.opened`; `emit` delivers a named event to it.
 */
export class FakeEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  static opened: FakeEventSource[] = [];

  readonly url: string;
  readyState = FakeEventSource.CONNECTING;
  private readonly listeners = new Map<string, Set<(event: Event) => void>>();

  constructor(url: string | URL) {
    this.url = String(url);
    FakeEventSource.opened.push(this);
  }

  addEventListener(type: string, listener: (event: Event) => void) {
    const set = this.listeners.get(type) ?? new Set();
    set.add(listener);
    this.listeners.set(type, set);
  }

  removeEventListener(type: string, listener: (event: Event) => void) {
    this.listeners.get(type)?.delete(listener);
  }

  close() {
    this.readyState = FakeEventSource.CLOSED;
  }

  /** Deliver `type` (`open`, `error`, or a named event with JSON `data`). */
  emit(type: string, data?: unknown) {
    if (type === "open") {
      this.readyState = FakeEventSource.OPEN;
    }
    const event =
      data === undefined ? new Event(type) : new MessageEvent(type, { data: JSON.stringify(data) });
    act(() => {
      for (const listener of this.listeners.get(type) ?? []) {
        listener(event);
      }
    });
  }

  /** The server refused it: the browser gives up (`lost`). */
  fail() {
    this.readyState = FakeEventSource.CLOSED;
    this.emit("error");
  }

  /** The newest one opened. */
  static latest(): FakeEventSource {
    const source = FakeEventSource.opened.at(-1);
    if (source === undefined) {
      throw new Error("No EventSource was opened");
    }
    return source;
  }
}

/** Use {@link FakeEventSource} for EventSource until the test ends. */
export function stubEventSource() {
  FakeEventSource.opened = [];
  vi.stubGlobal("EventSource", FakeEventSource);
}
