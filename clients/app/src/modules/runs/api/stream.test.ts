import { describe, expect, it } from "vitest";

import { MAX_KEPT_EVENTS, keepEvent, streamUrl, type RunEvent } from "./stream";

function line(seq: number): RunEvent {
  return {
    seq,
    at: "2026-10-31T12:00:00Z",
    layer: "agent",
    level: "info",
    message: `line ${String(seq)}`,
  };
}

describe("keepEvent", () => {
  it("keeps lines in place until the run's cap", () => {
    const kept: RunEvent[] = [];
    for (let seq = 1; seq <= MAX_KEPT_EVENTS; seq += 1) {
      expect(keepEvent(kept, line(seq))).toBe(true);
    }
    expect(keepEvent(kept, line(MAX_KEPT_EVENTS + 1))).toBe(false);
    expect(kept).toHaveLength(MAX_KEPT_EVENTS);
    expect(kept[0]?.seq).toBe(1);
  });
});

describe("streamUrl", () => {
  it("resumes after the last line seen", () => {
    expect(streamUrl("r1", 0)).toBe("/api/v1/runs/r1/events/stream");
    expect(streamUrl("r1", 7)).toBe("/api/v1/runs/r1/events/stream?after=7");
  });
});
